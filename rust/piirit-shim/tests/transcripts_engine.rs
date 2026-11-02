//! Whole path with real engine: tap on voice message → core's file decoded → whisper.cpp
//! (weightless test model) → outcome in `Transcript` and store. Stubbed or broken step ends
//! FAILED, which this refuses.

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;
#[allow(dead_code)]
#[path = "../src/transcripts/store.rs"]
mod store;

const PROBE_QML: &str = r#"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string seen: ''
        Transcriber { id: transcriber; enabled: true; languages: 'en'; mode: 0 }
        Transcript {
            id: voice
            account_id: 1
            message_id: 13
            onChanged: if (state === 2) seen += 'running;'
        }
        function report() {
            return transcriber.model_state + '|' + voice.state + '|' + voice.error + '|'
                + voice.language + '|' + seen
        }
        function tap() { voice.transcribe(false) }
    }
"#;

#[test]
fn a_tapped_voice_message_goes_through_decoder_and_engine() {
    let temp = common::temp_tree("transcripts-engine");
    let journal = common::fresh_journal(&temp);
    let speech = temp.join("speech");
    std::fs::create_dir_all(&speech).expect("create speech dir");
    let test_model = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/whisper.cpp/models/for-tests-ggml-tiny.bin");
    let model = speech.join("ggml-small-q5_1.bin");
    std::fs::copy(&test_model, &model).expect("place test model");
    let bytes = std::fs::metadata(&model).expect("model size").len();
    let voice = temp.join("voice.aac");
    piirit_video::synth_tone(&voice, 3, 48_000, 1, true).expect("make voice message");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        std::env::set_var("PIIRIT_FAKE_VOICE_FILE", &voice);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_SPEECH_LIBS", piirit_speech::BUILT_LIBS);
        // Model already on phone at this size: nothing fetched.
        std::env::set_var("PIIRIT_SPEECH_MODEL_URL", "http://127.0.0.1:9/none.bin");
        std::env::set_var("PIIRIT_SPEECH_MODEL_SHA256", "00");
        std::env::set_var("PIIRIT_SPEECH_MODEL_BYTES", bytes.to_string());
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
    // SAFETY for every block: run inside `exec()`; `engine` outlives it.
    let report = move || unsafe {
        QString::from_qvariant((*engine_ptr).invoke_method("report".into(), &[]))
            .map(|text| text.to_string())
            .unwrap_or_default()
    };
    let field = move |index: usize| {
        report()
            .split('|')
            .nth(index)
            .unwrap_or_default()
            .to_owned()
    };
    let last: Rc<RefCell<String>> = Rc::default();

    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    // Model ready (size matches) and core up: tap.
    plan.until(1, 30, move |last| {
        if field(0) != "2" && !last {
            return false;
        }
        unsafe { (*engine_ptr).invoke_method("tap".into(), &[]) };
        true
    });
    // Ended: done, no speech, other language or failed.
    {
        let last = last.clone();
        plan.until(1, 90, move |out_of_time| {
            let state = field(1);
            if !matches!(state.as_str(), "3" | "4" | "5" | "6") && !out_of_time {
                return false;
            }
            *last.borrow_mut() = report();
            unsafe { (*engine_ptr).quit() };
            true
        });
    }
    plan.run();
    engine.exec();

    let report = last.borrow().clone();
    let fields: Vec<&str> = report.split('|').collect();
    assert!(
        matches!(fields.get(1), Some(&"3" | &"4")),
        "tap ends done or no speech, not failed or stuck (model|state|error|language|seen): \
         {report}"
    );
    assert_eq!(fields.get(2), Some(&""), "no error: {report}");
    let calls = common::calls(&journal);
    assert!(
        calls
            .iter()
            .any(|(method, params)| method == "get_message" && params[1] == 13),
        "message read from core for its file: {calls:?}"
    );
    let rows = store::Store::open(&speech.join("transcripts.sqlite"))
        .expect("open store")
        .get(1, 13)
        .expect("read row");
    assert!(
        rows.is_some_and(|row| Some(row.state.to_string().as_str()) == fields.get(1).copied()),
        "outcome stored under (1, 13): {report}"
    );
}
