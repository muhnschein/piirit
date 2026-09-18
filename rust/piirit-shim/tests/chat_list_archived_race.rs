//! Opening the archived list must not show the ordinary chats.
//!
//! Pushing the page sets `account_id` and `archived` in whatever order QML
//! chooses, and each of them starts its own fetch -- one for the ordinary
//! listing, one for the archived. Both were applied on arrival, so the
//! slower answer won: the archived page showed ordinary chats until it was
//! backed out of and opened again.
//!
//! The fake core is told to answer the ordinary listing slowly, which
//! makes that ordering certain rather than a coin toss.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

// The order matters, and it is not arbitrary: `pageStack.push` applies
// its initial properties from a key-sorted map, so `accountId` reaches the
// page before `archived` does. The model therefore learns its account
// while still set to the ordinary listing, asks for that, and only then
// hears which list was actually wanted.
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
fn the_archived_list_is_not_overwritten_by_a_slow_ordinary_one() {
    let temp = std::env::temp_dir().join(format!("piirit-archived-race-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_JOURNAL", temp.join("journal.ndjson"));
        // Long enough to land well after the archived answer.
        std::env::set_var("PIIRIT_FAKE_CHATLIST_DELAY_MS", "1500");
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
        (*engine_ptr).invoke_method("open".into(), &[]);
    });

    // The archived answer is back; the ordinary one is still in flight.
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("early", call!("count")));
    });

    // Well after the slow one has landed.
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("late", call!("count")));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let listings: Vec<String> = common::calls(&temp.join("journal.ndjson"))
        .into_iter()
        .filter(|(name, _)| name == "get_chatlist_entries")
        .map(|(_, params)| params.to_string())
        .collect();
    let context = format!("steps: {steps:?}, listings: {listings:?}");

    // Without both requests in flight there is no race to survive, and
    // this test would pass on the broken code too.
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
