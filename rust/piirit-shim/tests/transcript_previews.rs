//! Chat-list row of a voice message previews its done transcript, else core's words: switched on
//! mid-load (stale read caught), off, on, and a new row written (re-read). Only voice rows re-read.

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::cell::RefCell;
use std::rc::Rc;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::json;

mod common;
#[allow(dead_code)]
#[path = "../src/transcripts/store.rs"]
mod store;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        Transcriber {
            id: transcriber
            enabled: false
            languages: 'en'
            mode: 0
        }
        Transcript { id: voice; account_id: 1; message_id: 13 }
        ChatList { id: chats; account_id: 1 }
        Repeater { id: rows; model: chats.rows; Item { property string said: model.chat_id + '=' + model.preview } }
        Connections {
            target: core
            onStatus_changed: if (core.status === 'ready') chats.reload()
        }
        function report() {
            var out = []
            for (var i = 0; i < rows.count; i++) {
                if (rows.itemAt(i)) out.push(rows.itemAt(i).said)
            }
            return out.join('|') + '#' + voice.state
        }
        function act(what) {
            if (what === 'on') transcriber.enabled = true
            else if (what === 'off') transcriber.enabled = false
            else if (what === 'tap') voice.transcribe(true)
        }
    }
";

type Log = Rc<RefCell<Vec<(&'static str, String)>>>;

/// Done transcript of voice message 13, written beside the app's own connection.
fn done_row(file: &std::path::Path) {
    let store = store::Store::open(file).expect("open store");
    store
        .put(&store::Row {
            account: 1,
            msg: 13,
            chat: 2,
            timestamp: 1_700_000_000,
            state: 3,
            language: "en".into(),
            model: "ggml-small-q5_1.bin".into(),
            // Preview is one line.
            text: "hello  there\nfriend".into(),
            error: String::new(),
            updated: 1_700_000_100,
        })
        .expect("row");
}

const HELD: &str = "get_chatlist_items_by_entries";
const SAID: &str = "1=last in 1|2=🎤 hello there friend";
const CORE: &str = "1=last in 1|2=🎤 Voice message";

#[test]
#[allow(clippy::too_many_lines)]
fn a_voice_row_previews_its_transcript_while_switched_on() {
    let temp = common::temp_tree("transcript-previews");
    let journal = common::fresh_journal(&temp);
    let speech = temp.join("speech");
    std::fs::create_dir_all(&speech).expect("create speech dir");
    std::fs::write(speech.join("ggml-small-q5_1.bin"), [0_u8; 16]).expect("place model");
    let file = speech.join("transcripts.sqlite");
    done_row(&file);
    let hold = temp.join("hold");
    std::fs::write(&hold, HELD).expect("hold first load");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_VOICE_LAST", "1");
        // Startup sweep keeps rows of listed profiles only.
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        std::env::set_var("PIIRIT_FAKE_HOLD", &hold);
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
    engine.load_data(QByteArray::from(PROBE_QML));
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let log: Log = Rc::default();
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
    // First load asked and held: switched on before its answer lands.
    {
        let journal = journal.clone();
        plan.until(1, 30, move |last| {
            if !common::methods(&journal).iter().any(|name| name == HELD) && !last {
                return false;
            }
            act("on");
            std::fs::remove_file(&hold).expect("release first load");
            true
        });
    }
    let mut step = |label: &'static str, want: &'static str, next: &'static str| {
        let (log, file) = (log.clone(), file.clone());
        plan.until(1, 30, move |last| {
            let now = report();
            if !now.starts_with(want) && !last {
                return false;
            }
            log.borrow_mut().push((label, now));
            if next == "quit" {
                unsafe { (*engine_ptr).quit() };
            } else if next == "restore, off" {
                done_row(&file);
                act("off");
            } else {
                act(next);
            }
            true
        });
    };
    // No engine here: tap ends failed, row replaced, list reads it again.
    step("on during load", SAID, "tap");
    // Off deletes the model: switch tested last, done row back first.
    step("failed", "1=last in 1|2=🎤 Voice message#6", "restore, off");
    step("off", CORE, "on");
    step("on", SAID, "quit");
    plan.run();
    engine.exec();

    let calls = common::calls(&journal);
    let log = log.borrow();
    let context = format!("log: {log:#?}\ncalls: {calls:#?}");
    let at = |label: &str| {
        log.iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.split('#').next().unwrap_or_default().to_owned())
    };
    assert_eq!(
        at("on during load").as_deref(),
        Some(SAID),
        "switched on while list loaded: answer read without transcripts must be read again\n{context}"
    );
    assert_eq!(
        at("off").as_deref(),
        Some(CORE),
        "switched off: core's words again\n{context}"
    );
    assert_eq!(
        at("on").as_deref(),
        Some(SAID),
        "switched on: transcript again\n{context}"
    );
    assert_eq!(
        at("failed").as_deref(),
        Some(CORE),
        "new row without text: list reads it, core's words\n{context}"
    );
    // Chat 1's last message isn't voice: read once, at load.
    let chat_one = calls
        .iter()
        .filter(|(name, params)| {
            name == HELD
                && params
                    .get(1)
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|ids| ids.contains(&json!(1)))
        })
        .count();
    assert_eq!(chat_one, 1, "a row without voice was read again\n{context}");
}
