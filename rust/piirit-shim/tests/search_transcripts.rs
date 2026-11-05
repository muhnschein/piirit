//! Search finds voice messages by transcript, as text messages: any case, newest first, once
//! when core found them too. Not when switched off, in chats core lists not, or from blocked
//! senders.

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

const DONE: u32 = 3;
const FAILED: u32 = 6;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property int answers: 0
        property int blocks: 0
        Transcriber { id: transcriber; enabled: true; languages: 'en'; mode: 0 }
        SearchResults {
            id: search
            account_id: 1
            onRows_changed: answers++
        }
        ContactList {
            id: contacts
            account_id: 1
            onBlocking_applied: blocks++
        }
        Repeater {
            id: rows
            model: search.rows
            Item {
                property string line: model.kind === 'message'
                    ? model.message_id + '@' + model.chat_id + '=' + model.subtitle : ''
            }
        }
        // Answers and blocks first: steps wait for it to grow.
        function report() {
            var out = (answers + blocks) + ':' + search.message_count + '/' + search.message_total + ':'
            for (var i = 0; i < rows.count; i++) {
                if (rows.itemAt(i).line !== '') { out += rows.itemAt(i).line + ';' }
            }
            return out
        }
        function act(what) {
            if (what === 'off') { transcriber.enabled = false; search.query = 'CAFÉ' }
            else if (what === 'on') { transcriber.enabled = true; search.query = 'café' }
            else if (what === 'block') { contacts.block(10) }
            else { search.query = what }
        }
    }
";

fn row(account: u32, msg: u32, chat: u32, state: u32, text: &str) -> store::Row {
    store::Row {
        account,
        msg,
        chat,
        timestamp: 1_700_000_000,
        state,
        language: "en".into(),
        model: "ggml-small-q5_1.bin".into(),
        text: text.into(),
        error: String::new(),
        updated: 1_700_000_100,
    }
}

type Log = Rc<RefCell<Vec<(&'static str, String)>>>;

#[test]
#[allow(clippy::too_many_lines)]
fn voice_messages_are_found_by_their_transcript() {
    let temp = common::temp_tree("search-transcripts");
    let journal = common::fresh_journal(&temp);
    let speech = temp.join("speech");
    std::fs::create_dir_all(&speech).expect("create speech dir");
    std::fs::write(speech.join("ggml-small-q5_1.bin"), [0_u8; 16]).expect("place model");
    {
        let store = store::Store::open(&speech.join("transcripts.sqlite")).expect("make store");
        // 13: voice message in group chat 2 (two lines); 30: in archived chat 3, text kind in
        // fake core: marked by transcript alone.
        store
            .put(&row(
                1,
                13,
                2,
                DONE,
                "Meet me at the Café,\n message 1 follows",
            ))
            .expect("row");
        store
            .put(&row(1, 30, 3, DONE, "café in the archive, message 1 too"))
            .expect("row");
        // Chat core doesn't list (blocked); failed run; other profile.
        store
            .put(&row(1, 14, 99, DONE, "café nobody may see"))
            .expect("row");
        store
            .put(&row(1, 15, 2, FAILED, "café half heard"))
            .expect("row");
        store
            .put(&row(2, 12, 2, DONE, "café of profile two"))
            .expect("row");
    }

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_MEDIA", "1");
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_SPEECH_LIBS", temp.join("no-libs"));
        // Model on phone already at this size; refetch (after off) goes nowhere.
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
    let log: Log = Rc::default();
    // SAFETY for every block: run inside `exec()`; `engine` outlives it.
    let invoke = move |name: &str| unsafe {
        QString::from_qvariant((*engine_ptr).invoke_method(name.into(), &[]))
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

    // Each step: act, then wait for the answer after it (counter past `start`).
    let answers = move || -> u32 {
        invoke("report")
            .split(':')
            .next()
            .and_then(|count| count.parse().ok())
            .unwrap_or(0)
    };
    let mut step = |label: &'static str, what: &'static str| {
        let log = log.clone();
        let before = Rc::new(RefCell::new(None::<u32>));
        plan.until(1, 30, move |last| {
            let mut before = before.borrow_mut();
            let Some(start) = *before else {
                *before = Some(answers());
                act(what);
                return false;
            };
            if answers() <= start && !last {
                return false;
            }
            log.borrow_mut().push((label, invoke("report")));
            true
        });
    };
    step("any case", "CAFÉ");
    step("merged", "message 1");
    step("off", "off");
    step("on", "on");
    step("block", "block");
    step("blocked", "Café");
    plan.after(1, move || unsafe { (*engine_ptr).quit() });
    plan.run();
    engine.exec();

    let log = log.borrow();
    let shown = |label: &str| -> String {
        let report = log
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, report)| report.clone())
            .unwrap_or_default();
        // Drop answer counter: varies with load.
        report
            .split_once(':')
            .map(|(_, rest)| rest.to_owned())
            .unwrap_or(report)
    };
    // Unicode case, archived chat too, newest first; core's text found nothing. Voice as chat
    // list previews it: mark, one line.
    assert_eq!(
        shown("any case"),
        "2/2:30@3=🎤 café in the archive, message 1 too;\
         13@2=🎤 Meet me at the Café, message 1 follows;",
        "{log:?}"
    );
    // Core found 1, 10, 12..15 by text: 13 once, with its own text (marked: voice); 30 by
    // transcript, in id order.
    assert_eq!(
        shown("merged"),
        "7/7:30@3=🎤 café in the archive, message 1 too;15@2=message 15;14@2=message 14;\
         13@2=🎤 message 13;12@2=message 12;10@2=message 10;1@1=message 1;",
        "{log:?}"
    );
    // Switched off: transcripts hidden, here too.
    assert_eq!(shown("off"), "0/0:", "{log:?}");
    assert_eq!(shown("on"), shown("any case"), "{log:?}");
    // Every seeded message is from contact 10.
    assert_eq!(shown("blocked"), "0/0:", "{log:?}");
}
