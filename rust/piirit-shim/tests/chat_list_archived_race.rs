//! Archived list must not show ordinary chats.
//!
//! QML sets `account_id` and `archived` in any order, each starting a fetch; slower answer wins.
//! Fake answers ordinary listing slowly to make that order certain.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

// `pageStack.push` sets properties from key-sorted map: `accountId` before `archived`.
// Model fetches account's ordinary list first, then hears archived was wanted.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatList { id: chats }
        function open() {
            chats.account_id = 1
            chats.archived = true
        }
        // The two listings differ in size -- one archived chat, two
        // ordinary ones -- so the count alone says which answer landed.
        function count() { return chats.count }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_archived_list_is_not_overwritten_by_a_slow_ordinary_one() {
    let temp = std::env::temp_dir().join(format!("piirit-archived-race-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // Ordinary listing answered only once archived one landed.
    let hold = temp.join("hold");
    std::fs::write(&hold, r#""get_chatlist_entries","params":[1,null"#)
        .expect("write the hold file");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_JOURNAL", temp.join("journal.ndjson"));
        std::env::set_var("PIIRIT_FAKE_HOLD", &hold);
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr) => {{
            let result = (*engine_ptr).invoke_method($name.into(), &[]);
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    let core_ptr = std::ptr::addr_of!(core_box);
    let journal = temp.join("journal.ndjson");
    let mut plan = common::Plan::default();

    plan.until(1, 30, move |_| unsafe { common::ready(&*core_ptr) });
    plan.after(0, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
        (*engine_ptr).invoke_method("open".into(), &[]);
    });

    // Archived answer back; ordinary one held.
    plan.until(1, 30, move |last| unsafe {
        if call!("count") != "1" && !last {
            return false;
        }
        (*steps_ptr).push(("early", call!("count")));
        let _ = std::fs::remove_file(&hold);
        true
    });

    // Ordinary answer let go, its rows asked for (list's last request): lands next.
    plan.until(1, 30, move |_| {
        common::methods(&journal)
            .iter()
            .filter(|name| *name == "get_chatlist_items_by_entries")
            .count()
            >= 2
    });
    // A late count of two is the bug: watch a while for it.
    plan.until(1, 3, move |last| unsafe {
        let count = call!("count");
        if count == "1" && !last {
            return false;
        }
        (*steps_ptr).push(("late", count));
        (*engine_ptr).quit();
        true
    });

    plan.run();
    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let asked = |method: &str| -> Vec<String> {
        common::calls(&temp.join("journal.ndjson"))
            .into_iter()
            .filter(|(name, _)| name == method)
            .map(|(_, params)| params.to_string())
            .collect()
    };
    let listings = asked("get_chatlist_entries");
    let rows = asked("get_chatlist_items_by_entries");
    let context = format!("steps: {steps:?}, listings: {listings:?}, rows: {rows:?}");

    // No overlap, no race: test would pass on broken code.
    assert_eq!(
        listings.len(),
        2,
        "the model did not ask for both listings, so nothing here is being \
         tested. {context}"
    );
    assert!(
        listings.iter().any(|params| params.contains("[1,null")),
        "no request for the ordinary listing was made. {context}"
    );
    assert!(
        listings.iter().any(|params| params.contains("[1,1,")),
        "no request for the archived listing was made. {context}"
    );

    assert_eq!(
        rows,
        ["[1,[3]]", "[1,[1,2]]"],
        "the ordinary listing was not answered after the archived one, so \
         this test is no longer arranged the way the bug needs. {context}"
    );
    assert_eq!(
        value("early"),
        "1",
        "the archived listing did not arrive first, so this test is no longer \
         arranged the way the bug needs. {context}"
    );
    assert_eq!(
        value("late"),
        "1",
        "a late answer to the ordinary listing replaced the archived one -- two \
         chats where the archived list holds one -- so the archived page shows \
         ordinary chats. {context}"
    );
}
