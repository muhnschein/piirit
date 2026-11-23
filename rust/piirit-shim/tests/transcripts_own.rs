//! Own voice messages (sent here or from other device) transcribe on their own like received
//! ones: on core's `MsgsChanged`, and with older messages. Engine stubbed: runs end failed.

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::cell::RefCell;
use std::rc::Rc;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r#"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        Transcriber {
            id: transcriber
            enabled: true
            languages: 'en'
            mode: 2
        }
        Transcript { id: sent; account_id: 1; message_id: 13 }
        Transcript { id: older; account_id: 2; message_id: 13 }
        function report() {
            return transcriber.model_state + '|' + sent.state + '|' + older.state
        }
        function act(what) {
            if (what === 'own message sent') transcriber.handle_event(1, 'MsgsChanged',
                '{"kind":"MsgsChanged","chatId":2,"msgId":13}')
            else if (what === 'older on charger') {
                transcriber.charging = true
                transcriber.older = true
            }
        }
    }
"#;

#[test]
fn own_voice_messages_transcribe_on_their_own() {
    let temp = common::temp_tree("transcripts-own");
    let journal = common::fresh_journal(&temp);
    let speech = temp.join("speech");
    std::fs::create_dir_all(&speech).expect("create speech dir");
    std::fs::write(speech.join("ggml-small-q5_1.bin"), [0_u8; 16]).expect("place model");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_MEDIA", "1");
        std::env::set_var("PIIRIT_FAKE_OWN_VOICE", "1");
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_SPEECH_LIBS", temp.join("no-libs"));
        std::env::set_var("PIIRIT_SPEECH_MODEL_URL", "http://127.0.0.1:9/none.bin");
        std::env::set_var("PIIRIT_SPEECH_MODEL_SHA256", "00");
        std::env::set_var("PIIRIT_SPEECH_MODEL_BYTES", "16");
    }
    std::fs::create_dir_all(temp.join("accounts")).expect("create accounts dir");

    piirit_shim::register_qml_types();
    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let log: Rc<RefCell<Vec<(&'static str, String)>>> = Rc::default();
    // SAFETY for every block: run inside `exec()`; `engine` outlives it.
    let report = move || unsafe {
        QString::from_qvariant((*engine_ptr).invoke_method("report".into(), &[]))
            .map(|text| text.to_string())
            .unwrap_or_default()
    };
    let act = move |what: &str| unsafe {
        (*engine_ptr).invoke_method("act".into(), &[QString::from(what).to_qvariant()]);
    };

    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    let mut step = |label: &'static str, wanted: &'static str, next: &'static str| {
        let log = log.clone();
        plan.until(1, 30, move |last| {
            if report() != wanted && !last {
                return false;
            }
            log.borrow_mut().push((label, report()));
            if next == "quit" {
                unsafe { (*engine_ptr).quit() };
            } else {
                act(next);
            }
            true
        });
    };
    step("model ready", "2|0|0", "own message sent");
    step("sent", "2|6|0", "older on charger");
    step("older", "2|6|6", "quit");
    plan.run();
    engine.exec();

    let log = log.borrow();
    let at = |label: &str| {
        log.iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(
        at("model ready"),
        Some("2|0|0"),
        "nothing runs before a message is sent. {log:?}"
    );
    assert_eq!(
        at("sent"),
        Some("2|6|0"),
        "an own voice message just sent is transcribed without a tap (stub engine: failed). {log:?}"
    );
    assert_eq!(
        at("older"),
        Some("2|6|6"),
        "an older own voice message is transcribed with the older ones. {log:?}"
    );
}
