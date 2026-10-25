//! Clean-links on: sent link drops tracking params.
//!
//! Dconf setting. Model rewrites text before core; peer and own bubble show cleaned link.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const DIRTY: &str = "see https://example.org/page?utm_source=mail&id=7&fbclid=x now";
const CLEAN: &str = "see https://example.org/page?id=7 now";

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        function sendAsTyped(text) {
            chat.clean_links = false
            chat.send(text)
            return 'ok'
        }
        function sendCleaned(text) {
            chat.clean_links = true
            chat.send(text)
            return 'ok'
        }
    }
";

#[test]
fn links_are_cleaned_on_the_way_out_only_when_asked() {
    let temp = std::env::temp_dir().join(format!("piirit-clean-links-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
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

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    plan.after(2, move || unsafe {
        call!("sendAsTyped", QString::from(DIRTY));
    });
    plan.after(2, move || unsafe {
        call!("sendCleaned", QString::from(DIRTY));
    });
    plan.after(2, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let sent: Vec<String> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "misc_send_msg")
        .filter_map(|(_, params)| {
            params
                .pointer("/2")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect();
    assert_eq!(
        sent,
        vec![DIRTY.to_string(), CLEAN.to_string()],
        "the first send should go as typed and the second with its link cleaned"
    );
}
