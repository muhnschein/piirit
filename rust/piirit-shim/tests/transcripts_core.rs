//! Transcripts against the fake core: rows of gone messages and profiles leave the store
//! (startup sweep, profile removal, `MsgDeleted`), taps and arrivals run on the speech thread
//! (engine stubbed here: they end failed, with a reason), battery saving holds arrivals, a
//! job waits while the model is missing, switching off hides rows, drops the queue and deletes
//! the model.

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
        property string counted: ''
        Transcriber {
            id: transcriber
            enabled: true
            languages: 'en'
            mode: 0
            onChat_counted: counted += account_id + '/' + chat_id + '=' + count + ';'
        }
        Transcript { id: first; account_id: 1; message_id: 13 }
        Transcript { id: twin; account_id: 1; message_id: 13 }
        Transcript { id: other; account_id: 2; message_id: 13 }
        ChatMedia { id: sounds; account_id: 1; chat_id: 2; kind: 'audio' }
        Connections {
            target: core
            onCore_event: transcriber.handle_event(context_id, kind, payload_json)
        }
        function view(t) {
            return t.state + ':' + (t.manual ? 'm' : 'a') + ':' + t.text + ':'
                + (t.error !== '' ? 'error' : '')
        }
        function report() {
            return transcriber.model_state + '|' + transcriber.transcript_count + '|'
                + view(first) + '|' + view(twin) + '|' + view(other) + '|' + counted
        }
        function act(what) {
            if (what === 'count 1') transcriber.count_chat(1, 2)
            else if (what === 'count 2') transcriber.count_chat(2, 2)
            else if (what === 'remove profile 2') core.remove_account(2)
            else if (what === 'delete message') sounds.delete_message(13)
            else if (what === 'tap') first.transcribe(false)
            else if (what === 'tap any language') first.transcribe(true)
            else if (what === 'cancel') first.cancel()
            else if (what === 'arrival in battery saving') {
                transcriber.mode = 2
                transcriber.power_saving = true
                transcriber.handle_event(2, 'IncomingMsg',
                    '{"kind":"IncomingMsg","chatId":2,"msgId":13}')
            }
            else if (what === 'battery saving off') transcriber.power_saving = false
            else if (what === 'changed again') transcriber.handle_event(2, 'MsgsChanged',
                '{"kind":"MsgsChanged","chatId":2,"msgId":13}')
            else if (what === 'off') transcriber.enabled = false
            else if (what === 'on') transcriber.enabled = true
            else if (what === 'delete all') transcriber.delete_all()
        }
    }
"#;

fn row(account: u32, msg: u32, text: &str) -> store::Row {
    store::Row {
        account,
        msg,
        chat: 2,
        timestamp: 1_700_000_000,
        state: 3,
        language: "en".into(),
        model: "ggml-small-q5_1.bin".into(),
        text: text.into(),
        error: String::new(),
        updated: 1_700_000_100,
    }
}

/// Rows as `account/msg=state`, read beside the app's own connection.
fn rows(file: &Path) -> String {
    let Ok(connection) = rusqlite::Connection::open(file) else {
        return "no store".into();
    };
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .ok();
    let Ok(mut statement) =
        connection.prepare("SELECT account, msg, state FROM transcripts ORDER BY account, msg")
    else {
        return "no table".into();
    };
    statement
        .query_map([], |row| {
            Ok(format!(
                "{}/{}={}",
                row.get::<_, u32>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, u32>(2)?
            ))
        })
        .map(|found| found.filter_map(Result::ok).collect::<Vec<_>>().join(","))
        .unwrap_or_default()
}

type Log = Rc<RefCell<Vec<(&'static str, String)>>>;

#[test]
#[allow(clippy::too_many_lines)]
fn transcripts_follow_messages_profiles_settings_and_power() {
    let temp = common::temp_tree("transcripts-core");
    let journal = common::fresh_journal(&temp);
    let speech = temp.join("speech");
    std::fs::create_dir_all(&speech).expect("create speech dir");
    let model = speech.join("ggml-small-q5_1.bin");
    std::fs::write(&model, [0_u8; 16]).expect("place model");
    let file = speech.join("transcripts.sqlite");
    {
        let store = store::Store::open(&file).expect("make store");
        store.put(&row(1, 13, "hello there")).expect("row");
        store.put(&row(2, 13, "second profile")).expect("row");
        // Message deleted while app was closed.
        store.put(&row(1, 999, "gone message")).expect("row");
        // Profile removed while app was closed.
        store.put(&row(7, 13, "gone profile")).expect("row");
    }

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_MEDIA", "1");
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
        std::env::set_var("PIIRIT_FAKE_MSG_DELETED", "1");
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
    let report = move || unsafe {
        QString::from_qvariant((*engine_ptr).invoke_method("report".into(), &[]))
            .map(|text| text.to_string())
            .unwrap_or_default()
    };
    let act = move |what: &str| unsafe {
        (*engine_ptr).invoke_method("act".into(), &[QString::from(what).to_qvariant()]);
    };
    // Field of report: 0 model, 1 count, 2 first, 3 twin, 4 other, 5 counted.
    let field = move |index: usize| {
        report()
            .split('|')
            .nth(index)
            .unwrap_or_default()
            .to_owned()
    };

    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    // Each step: wait for `ready`, log report and rows under `label`, then `next` action.
    let mut step = |label: &'static str, ready: Box<dyn Fn() -> bool>, next: &'static str| {
        let (log, file) = (log.clone(), file.clone());
        plan.until(1, 30, move |last| {
            if !ready() && !last {
                return false;
            }
            log.borrow_mut().push((label, report()));
            log.borrow_mut().push((label, rows(&file)));
            if next == "quit" {
                unsafe { (*engine_ptr).quit() };
            } else {
                act(next);
            }
            true
        });
    };
    step(
        "swept",
        Box::new(move || field(0) == "2" && field(1) == "2" && field(4).starts_with('3')),
        "count 1",
    );
    step(
        "counted with row",
        Box::new(move || field(5).contains("1/2=")),
        "remove profile 2",
    );
    step(
        "profile removed",
        Box::new(move || field(1) == "1" && field(4).starts_with('0')),
        "count 2",
    );
    step(
        "counted without row",
        Box::new(move || field(5).contains("2/2=")),
        "delete message",
    );
    step(
        "message deleted",
        Box::new(move || field(1) == "0" && field(2).starts_with('0')),
        "tap",
    );
    step(
        "tapped",
        Box::new(move || field(2).starts_with('6') && field(3).starts_with('6')),
        "arrival in battery saving",
    );
    step(
        "arrival held",
        Box::new(move || field(4).starts_with('1')),
        "battery saving off",
    );
    step(
        "arrival ran",
        Box::new(move || field(4).starts_with('6')),
        "off",
    );
    step(
        "off",
        Box::new(move || field(0) == "0" && field(2).starts_with('0') && field(4).starts_with('0')),
        "on",
    );
    step(
        "on without model",
        Box::new(move || field(0) == "3" && field(2).starts_with('6')),
        "tap any language",
    );
    step(
        "waits for model",
        Box::new(move || field(2).starts_with('1')),
        "off",
    );
    step(
        "off while waiting",
        Box::new(move || field(0) == "0" && field(2).starts_with('0')),
        "on",
    );
    // Off forgot this session's seen messages: only the store knows 2/13 is done.
    step(
        "on again",
        Box::new(move || field(0) == "3" && field(2).starts_with('6')),
        "changed again",
    );
    step(
        "changed again",
        Box::new(move || field(4).starts_with('6')),
        "tap any language",
    );
    step(
        "waits again",
        Box::new(move || field(2).starts_with('1')),
        "cancel",
    );
    step(
        "cancelled",
        Box::new(move || field(2).starts_with('6')),
        "delete all",
    );
    step(
        "all deleted",
        Box::new(move || field(2).starts_with('0') && field(4).starts_with('0')),
        "quit",
    );
    plan.run();
    engine.exec();

    check(&log.borrow(), &common::calls(&journal), &model);
}

/// Each step's report and rows as expected.
#[allow(clippy::too_many_lines)]
fn check(log: &[(&'static str, String)], calls: &[(String, serde_json::Value)], model: &Path) {
    let at = |label: &str| -> Vec<String> {
        log.iter()
            .filter(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .collect()
    };
    let context = format!(
        "log: {log:#?}\ncalls: {:?}",
        calls.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );
    let expect = |label: &str, report: &str, stored: &str, why: &str| {
        assert_eq!(
            at(label),
            vec![report.to_owned(), stored.to_owned()],
            "{label}: {why}\n{context}"
        );
    };

    // Model at right size: ready without fetching. Sweep keeps live rows only.
    expect(
        "swept",
        "2|2|3:a:hello there:|3:a:hello there:|3:a:second profile:|",
        "1/13=3,2/13=3",
        "rows of a deleted message and a removed profile go at start; live ones show",
    );
    assert!(
        calls
            .iter()
            .any(|(name, params)| name == "get_existing_msg_ids"
                && params == &serde_json::json!([1, [13, 999]])),
        "the sweep asked the core which stored messages still exist. {context}"
    );
    expect(
        "counted with row",
        "2|2|3:a:hello there:|3:a:hello there:|3:a:second profile:|1/2=0;",
        "1/13=3,2/13=3",
        "a chat whose voice message has a transcript has none left to do",
    );
    expect(
        "profile removed",
        "2|1|3:a:hello there:|3:a:hello there:|0:a::|1/2=0;",
        "1/13=3",
        "removing a profile drops its transcripts",
    );
    expect(
        "counted without row",
        "2|1|3:a:hello there:|3:a:hello there:|0:a::|1/2=0;2/2=1;",
        "1/13=3",
        "a voice message without transcript is counted",
    );
    expect(
        "message deleted",
        "2|0|0:a::|0:a::|0:a::|1/2=0;2/2=1;",
        "",
        "a deleted message's transcript goes, from every view",
    );
    expect(
        "tapped",
        "2|0|6:a::error|6:a::error|0:a::|1/2=0;2/2=1;",
        "1/13=6",
        "a tap runs on the speech thread; stub engine fails it with a reason, stored",
    );
    expect(
        "arrival held",
        "2|0|6:a::error|6:a::error|1:a::|1/2=0;2/2=1;",
        "1/13=6",
        "an arriving voice message queues in Always but battery saving holds it",
    );
    expect(
        "arrival ran",
        "2|0|6:a::error|6:a::error|6:a::error|1/2=0;2/2=1;",
        "1/13=6,2/13=6",
        "battery saving off: the held arrival runs",
    );
    expect(
        "off",
        "0|0|0:a::|0:a::|0:a::|1/2=0;2/2=1;",
        "1/13=6,2/13=6",
        "switched off: model deleted, transcripts hidden but kept",
    );
    assert!(
        !model.exists(),
        "switching off deletes the model file. {context}"
    );
    let on = at("on without model");
    assert_eq!(
        on.first()
            .map(|report| report.split('|').skip(2).collect::<Vec<_>>().join("|")),
        Some("6:a::error|6:a::error|6:a::error|1/2=0;2/2=1;".to_owned()),
        "switched on again: rows show again, model fetch failed (nowhere to fetch). {context}"
    );
    expect(
        "waits for model",
        "3|0|1:m::|1:m::|6:a::error|1/2=0;2/2=1;",
        "1/13=6,2/13=6",
        "a tap while the model is missing waits queued",
    );
    expect(
        "on again",
        "3|0|6:a::error|6:a::error|6:a::error|1/2=0;2/2=1;",
        "1/13=6,2/13=6",
        "switching off dropped the waiting tap: stored outcome shows, nothing queued",
    );
    let asked = calls
        .iter()
        .filter(|(name, params)| name == "get_message" && params == &serde_json::json!([2, 13]))
        .count();
    assert_eq!(
        asked, 1,
        "a message with a stored outcome changing again is not looked at or queued again. {context}"
    );
    expect(
        "waits again",
        "3|0|1:m::|1:m::|6:a::error|1/2=0;2/2=1;",
        "1/13=6,2/13=6",
        "a new tap queues again",
    );
    expect(
        "cancelled",
        "3|0|6:a::error|6:a::error|6:a::error|1/2=0;2/2=1;",
        "1/13=6,2/13=6",
        "cancel takes it off the queue; stored outcome shows again",
    );
    expect(
        "all deleted",
        "3|0|0:a::|0:a::|0:a::|1/2=0;2/2=1;",
        "",
        "delete all empties the store",
    );
}
