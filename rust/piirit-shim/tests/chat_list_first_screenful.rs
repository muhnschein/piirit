//! First load: screen's rows come first. Core builds items one by one; a long list at once
//! leaves page empty until the last is built.

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatList { id: chats; account_id: 1 }
        Repeater {
            id: rows
            model: chats.rows
            Item { property int cid: model.chat_id }
        }
        function count() { return '' + rows.count }
        function first() { return rows.count > 0 ? '' + rows.itemAt(0).cid : '' }
    }
";

#[test]
fn a_long_list_shows_its_first_screenful_before_the_rest() {
    let temp = std::env::temp_dir().join(format!("piirit-chat-list-first-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // Rest of the list (from 21st chat, id 219) held till first screenful is checked.
    let hold = temp.join("hold");
    std::fs::write(
        &hold,
        r#""get_chatlist_items_by_entries","params":[1,[219,"#,
    )
    .expect("write the hold file");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // 32 chats: 1, 2, 201..=230.
        std::env::set_var("PIIRIT_FAKE_MANY_CHATS", "30");
        std::env::set_var("PIIRIT_FAKE_HOLD", &hold);
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr) => {
            QString::from_qvariant((*engine_ptr).invoke_method($name.into(), &[]))
                .map(|text| text.to_string())
                .unwrap_or_default()
        };
    }

    let core_ptr = std::ptr::addr_of!(core_box);
    let mut plan = common::Plan::default();

    // SAFETY for every block: fire only inside `exec()`; `engine`, `core_box`, `steps` outlive it.
    plan.until(1, 30, move |_| unsafe { common::ready(&*core_ptr) });
    plan.after(0, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    plan.until(1, 30, move |last| unsafe {
        if call!("count") != "20" && !last {
            return false;
        }
        (*steps_ptr).push(("early-count", call!("count")));
        (*steps_ptr).push(("early-first", call!("first")));
        let _ = std::fs::remove_file(&hold);
        true
    });
    plan.until(1, 30, move |last| unsafe {
        if call!("count") != "32" && !last {
            return false;
        }
        (*steps_ptr).push(("count", call!("count")));
        (*steps_ptr).push(("first", call!("first")));
        (*engine_ptr).quit();
        true
    });

    plan.run();
    engine.exec();

    let calls = common::calls(&journal);
    let fetches: Vec<usize> = calls
        .iter()
        .filter(|(name, _)| name == "get_chatlist_items_by_entries")
        .map(|(_, params)| {
            params
                .pointer("/1")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        })
        .collect();
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}, fetches: {fetches:?}");

    assert_eq!(
        fetches,
        vec![20, 12],
        "the first load did not ask for one screenful and then the rest. {context}"
    );
    assert_eq!(
        (value("early-count").as_str(), value("early-first").as_str()),
        ("20", "1"),
        "the first screenful was not shown, in order, before the rest was read. {context}"
    );
    assert_eq!(
        (value("count").as_str(), value("first").as_str()),
        ("32", "1"),
        "the rest of the list did not follow the first screenful. {context}"
    );
}
