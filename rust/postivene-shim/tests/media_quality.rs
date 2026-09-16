//! The outgoing media quality reaches every account the core has.
//!
//! One setting for the whole app, held in dconf, and the core's
//! `media_quality` is per account: so the core object writes it to each
//! account when it is set, and again whenever the account list is read,
//! which is how a profile added later gets it too. The same arrangement
//! as the download limit, for the same reason; see `download_limit.rs`.
//!
//! It is the only thing the setting does. The core reads it while
//! recoding a picture it is about to send, which is the one moment the
//! app's pictures pass through -- see `media.rs` and the recoding pinned
//! against the real core in `deltachat-jsonrpc/tests/real_server.rs`.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        // Handed over before the core is up, as the app's root does.
        Component.onCompleted: core.media_quality = 0
        Timer {
            id: poll
            interval: 200; running: true; repeat: true
            onTriggered: {
                if (core.status === 'ready') {
                    poll.running = false
                    core.add_account()
                }
            }
        }
        Connections {
            target: core
            onAccount_added: core.refresh_accounts()
        }
        function change(quality) { core.media_quality = quality; return 'ok' }
    }
";

#[test]
fn the_quality_is_written_to_each_account_when_set_and_when_the_list_is_read() {
    let temp = std::env::temp_dir().join(format!("postivene-media-quality-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

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

    // Changed once the first write has had time to land. Balanced is the
    // default, so this is also the case a plain `0` has to survive: it is
    // handed over rather than left unsaid, and the core is told.
    single_shot(Duration::from_secs(4), move || unsafe {
        call!("change", 1);
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let calls = common::calls(&journal);
    let written: Vec<(u64, String)> = calls
        .iter()
        .filter(|(method, params)| {
            method == "set_config"
                && params.pointer("/1").and_then(Value::as_str) == Some("media_quality")
        })
        .filter_map(|(_, params)| {
            Some((
                params.pointer("/0").and_then(Value::as_u64)?,
                params.pointer("/2").and_then(Value::as_str)?.to_string(),
            ))
        })
        .collect();
    assert_eq!(
        written,
        vec![(1, "0".to_string()), (1, "1".to_string())],
        "expected the quality written to account 1 when the list was read, \
         and again when it changed: {calls:?}"
    );
}
