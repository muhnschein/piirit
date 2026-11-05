//! Chat with voice message last restates its notification once transcript shows in row, and again
//! once core's words are back: switched on mid-load (stale read caught), off, on. Chat 1, no voice:
//! never restated.

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::cell::RefCell;
use std::rc::Rc;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;
#[allow(dead_code)]
#[path = "../src/transcripts/store.rs"]
mod store;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string restated: ''
        Transcriber {
            id: transcriber
            enabled: false
            languages: 'en'
            mode: 0
        }
        ChatList {
            id: chats
            account_id: 1
            onMessage_restated: restated += chat_id + '=' + preview + '|'
        }
        Connections {
            target: core
            onStatus_changed: if (core.status === 'ready') chats.reload()
        }
        function heard() { return restated }
        function act(what) { transcriber.enabled = what === 'on' }
    }
";

type Log = Rc<RefCell<Vec<(&'static str, String)>>>;

/// Done transcript of voice message 13, last in chat 2.
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
            text: "hello there".into(),
            error: String::new(),
            updated: 1_700_000_100,
        })
        .expect("row");
}

const HELD: &str = "get_chatlist_items_by_entries";
const SAID: &str = "2=🎤 hello there|";
const CORE: &str = "2=🎤 Voice message|";

#[test]
fn a_voice_chat_restates_its_notification_when_its_preview_changes() {
    let temp = common::temp_tree("transcript-restates");
    let journal = common::fresh_journal(&temp);
    let speech = temp.join("speech");
    std::fs::create_dir_all(&speech).expect("create speech dir");
    std::fs::write(speech.join("ggml-small-q5_1.bin"), [0_u8; 16]).expect("place model");
    done_row(&speech.join("transcripts.sqlite"));
    let hold = temp.join("hold");
    std::fs::write(&hold, HELD).expect("hold first load");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_VOICE_LAST", "1");
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
    let heard = move || unsafe {
        QString::from_qvariant((*engine_ptr).invoke_method("heard".into(), &[]))
            .map(|text| text.to_string())
            .unwrap_or_default()
    };
    let act = move |what: &str| unsafe {
        (*engine_ptr).invoke_method("act".into(), &[QString::from(what).to_qvariant()]);
    };

    let mut plan = common::Plan::default();
    // First load asked and held: switched on before its answer lands.
    plan.until(1, 30, move |last| {
        if !common::methods(&journal).iter().any(|name| name == HELD) && !last {
            return false;
        }
        act("on");
        std::fs::remove_file(&hold).expect("release first load");
        true
    });
    let mut step = |label: &'static str, want: &'static str, next: &'static str| {
        let log = log.clone();
        plan.until(1, 30, move |last| {
            let now = heard();
            if !now.ends_with(want) && !last {
                return false;
            }
            log.borrow_mut().push((label, now));
            if next == "quit" {
                unsafe { (*engine_ptr).quit() };
            } else {
                act(next);
            }
            true
        });
    };
    step("on during load", SAID, "off");
    step("off", CORE, "on");
    step("on", SAID, "quit");
    plan.run();
    engine.exec();

    let log = log.borrow();
    let context = format!("log: {log:#?}");
    let at = |label: &str| {
        log.iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    assert!(
        at("on during load").ends_with(SAID),
        "switched on while list loaded: notification must get transcript once row re-read\n{context}"
    );
    assert!(
        at("off").ends_with(CORE),
        "switched off: notification back to core's words\n{context}"
    );
    assert!(
        at("on").ends_with(SAID),
        "switched on: notification gets transcript again\n{context}"
    );
    assert!(
        !at("on").contains("1="),
        "chat without voice restated\n{context}"
    );
}
