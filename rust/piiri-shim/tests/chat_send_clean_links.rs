//! With the setting on, a link goes out without its tracking parameters.
//!
//! The setting is the reader's and lives in dconf; the page hands it to
//! the model, and the model rewrites the text on the way out -- so what
//! reaches the core, and so the other end, is the cleaned link, and the
//! reader sees the same in their own bubble. With it off, the text goes
//! as typed.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piiri_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const DIRTY: &str = "see https://example.org/page?utm_source=mail&id=7&fbclid=x now";
const CLEAN: &str = "see https://example.org/page?id=7 now";

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
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
    let temp = std::env::temp_dir().join(format!("piiri-clean-links-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piiri_shim::register_qml_types();

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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        call!("sendAsTyped", QString::from(DIRTY));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        call!("sendCleaned", QString::from(DIRTY));
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        (*engine_ptr).quit();
    });

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
