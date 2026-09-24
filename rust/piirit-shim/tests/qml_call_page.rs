//! A call ringing in, answered on its page, and ended -- as the phone
//! would show it, and as the platform is told.
//!
//! `CallCenter.qml` is the window's: it holds the one call, pushes the
//! call page over whatever is on screen, and tells the platform what the
//! call is doing. None of the platform is here -- ngfd, which rings; mce,
//! which lights the screen; the notification service; the browser engine
//! -- so each is a stub that records what it was asked, and what is
//! checked is that each was asked the right thing at the right moment:
//! ringing while it rings and not after, the microphone granted to the
//! call's own page and taken back, the phone kept awake exactly while a
//! call is up, and a call that rang out left behind as a missed one.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack` as the call page uses it: it pops itself once
/// the call is over. Recorded rather than performed.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    pop: qt_method!(fn(&mut self)),
}

impl StackProbe {
    fn pop(&mut self) {
        let current = self.log.to_string();
        self.log = format!("{current}pop;").into();
        self.log_changed();
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Sailfish.WebEngine 1.0
    Item {
        Loader { id: centerLoader }
        Loader { id: pageHolder; width: 540; height: 960 }
        // The stack the call page is pushed onto.
        QtObject {
            id: stack
            property bool busy: false
            property int pushes: 0
            function push(url, props) {
                pushes += 1
                pageHolder.setSource(url, props)
                return pageHolder.item
            }
        }
        function load(url) {
            centerLoader.setSource(url, { stack: stack, enabled: true })
            if (centerLoader.status !== Loader.Ready) { return 'load-failed' }
            // ngfd numbers the event it plays.
            find(centerLoader.item, 'feedback').reply = 42
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            if (node.item && node.item !== node) {
                return findIn(node.item, name)
            }
            return null
        }
        function find(root, name) { return findIn(root, name) }
        // Anything on the page or the window's call, by name.
        function get(name, property) {
            var hit = findIn(pageHolder.item, name) || findIn(centerLoader.item, name)
            if (!hit) { return 'missing:' + name }
            return '' + hit[property]
        }
        function tap(name) {
            var hit = findIn(pageHolder.item, name)
            if (!hit) { return 'missing:' + name }
            hit.clicked()
            return 'ok'
        }
        function actions(name) {
            var note = findIn(centerLoader.item, name)
            if (!note) { return 'missing:' + name }
            var names = []
            for (var i = 0; i < note.remoteActions.length; i++) {
                names.push(note.remoteActions[i].name)
            }
            return names.join(',')
        }
        function pushes() { return '' + stack.pushes }
        function engine() { return WebEngine.notified }
        function callState() {
            var call = centerLoader.item.call
            return call.state + '|' + call.end_reason
        }
        // The other end hangs up, in the core's own words.
        function endedThere() {
            var call = centerLoader.item.call
            call.handle_event(1, 'CallEnded', JSON.stringify({
                kind: 'CallEnded', msg_id: call.message_id, chat_id: 1
            }))
            return call.state
        }
        function ring(messageId) {
            centerLoader.item.call.handle_event(1, 'IncomingCall', JSON.stringify({
                kind: 'IncomingCall', msg_id: messageId, chat_id: 1,
                place_call_info: 'v=0 other', has_video: false
            }))
            return centerLoader.item.call.state
        }
        function endRinging(messageId) {
            centerLoader.item.call.handle_event(1, 'CallEnded', JSON.stringify({
                kind: 'CallEnded', msg_id: messageId, chat_id: 1
            }))
            return centerLoader.item.call.state
        }
        // The page going, as a pop takes it.
        function dropPage() { pageHolder.setSource('', {}); return 'ok' }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_call_rings_is_answered_on_its_page_and_ends_with_the_platform_told() {
    let temp = std::env::temp_dir().join(format!("piirit-call-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_INCOMING_CALL_MS", "2000");
    }

    piirit_shim::register_qml_types();
    common::register_dbus_enum();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(StackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let stack_ptr = std::ptr::addr_of!(stack_box);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("CallCenter.qml"))
            )
        );
        // Nothing yet: nothing rings, nothing is kept awake, and mce is
        // not told about a call that is not there.
        record!("idle-awake", get!("callKeepAlive", "enabled"));
        record!("idle-mce", get!("mce", "called"));
    });

    // Ringing.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("pushes", call!("pushes"));
        record!("status", get!("callStatus", "text"));
        record!("ringing-buttons", get!("ringingButtons", "visible"));
        record!("hang-up-button", get!("hangUpButton", "visible"));
        record!("ring-published", get!("ringNote", "isPublished"));
        record!("ring-urgency", get!("ringNote", "urgency"));
        record!("ring-actions", call!("actions", QString::from("ringNote")));
        record!("ringing-feedback", get!("feedback", "called"));
        record!("ringing-mce", get!("mce", "called"));
        record!("ringing-awake", get!("callKeepAlive", "enabled"));
        record!("ringing-lit", get!("callDisplay", "preventBlanking"));
        record!("back", get!("callWho", "visible"));
        record!("answer", call!("tap", QString::from("answerButton")));
    });

    // Answered: the page is up, granted the microphone, and nothing rings.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("answered", call!("callState"));
        record!("view", get!("callView", "url"));
        record!("engine", call!("engine"));
        record!("answered-feedback", get!("feedback", "called"));
        record!("answered-mce", get!("mce", "called"));
        record!("ring-after", get!("ringNote", "isPublished"));
        record!("answered-lit", get!("callDisplay", "preventBlanking"));
        record!("answered-awake", get!("callKeepAlive", "enabled"));
        record!("ringing-buttons-after", get!("ringingButtons", "visible"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("ended", call!("endedThere"));
        record!("ended-status", get!("callStatus", "text"));
        record!("ended-view", get!("callViewLoader", "status"));
        record!("revoked", call!("engine"));
    });

    // Said for a moment, then gone.
    single_shot(Duration::from_secs(7), move || unsafe {
        record!("popped", (*stack_ptr).pinned().borrow().log.to_string());
        record!("after", call!("callState"));
        record!("after-mce", get!("mce", "called"));
        record!("after-awake", get!("callKeepAlive", "enabled"));
        call!("dropPage");
    });

    // A call that rings out, once the last page has gone.
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("ring-again", call!("ring", 9100));
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("pushes-again", call!("pushes"));
        record!("rang-out", call!("endRinging", 9100));
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        record!("missed", call!("callState"));
        record!("missed-published", get!("missedNote", "isPublished"));
        record!("missed-category", get!("missedNote", "category"));
        record!("missed-body", get!("missedNote", "body"));
        record!(
            "missed-actions",
            call!("actions", QString::from("missedNote"))
        );
        record!("missed-ring", get!("ringNote", "isPublished"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the call center did not load. {context}"
    );
    assert_eq!(value("idle-awake"), "false", "{context}");
    assert_eq!(
        value("idle-mce"),
        "",
        "mce was told about a call before there was one. {context}"
    );

    assert_eq!(
        value("pushes"),
        "1",
        "a ringing call did not bring up its page. {context}"
    );
    assert_eq!(value("status"), "Incoming call", "{context}");
    assert_eq!(value("ringing-buttons"), "true", "{context}");
    assert_eq!(value("hang-up-button"), "false", "{context}");
    assert_eq!(
        value("ring-published"),
        "true",
        "a ringing call raised no notification. {context}"
    );
    assert_eq!(
        value("ring-urgency"),
        "2",
        "a ringing call is not the critical notification a ringing phone is. {context}"
    );
    assert_eq!(value("ring-actions"), "default,decline,answer", "{context}");
    assert!(
        value("ringing-feedback").contains("Play [{\"type\":\"s\",\"value\":\"voip_ringtone\"}"),
        "the phone was not asked to ring with its own VoIP ringtone: {}. {context}",
        value("ringing-feedback")
    );
    assert!(
        value("ringing-mce").contains("\"value\":\"ringing\""),
        "mce was not told a call is ringing: {}. {context}",
        value("ringing-mce")
    );
    assert_eq!(value("ringing-awake"), "true", "{context}");
    assert_eq!(value("ringing-lit"), "true", "{context}");
    assert_eq!(value("answer"), "ok", "{context}");

    assert!(
        value("answered").starts_with("starting|") || value("answered").starts_with("connecting|"),
        "answering on the page did not answer the call: {}. {context}",
        value("answered")
    );
    let view = value("view");
    assert!(
        view.starts_with("http://127.0.0.1:") && view.contains("#acceptCall="),
        "the call's page is not where the call is being answered: {view}. {context}"
    );
    let authority = view
        .strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default()
        .to_string();
    let engine_said = value("engine");
    assert!(
        engine_said.contains("embedui:perms")
            && engine_said.contains("\"msg\":\"add\"")
            && engine_said.contains("\"type\":\"microphone\"")
            && engine_said.contains(&format!("\"uri\":\"http://{authority}\"")),
        "the call's page -- and only its origin -- was not granted the \
         microphone before it asked: {engine_said}. {context}"
    );
    assert!(
        value("answered-feedback").contains("Stop [{\"type\":\"u\",\"value\":42}]"),
        "the ringtone ngfd numbered was not stopped on answering: {}. {context}",
        value("answered-feedback")
    );
    assert!(
        value("answered-mce")
            .trim_end()
            .ends_with("\"value\":\"active\"},{\"type\":\"s\",\"value\":\"normal\"}]"),
        "mce was not told a call is up: {}. {context}",
        value("answered-mce")
    );
    assert_eq!(
        value("ring-after"),
        "false",
        "the ringing notification outlived the ringing. {context}"
    );
    assert_eq!(value("answered-lit"), "false", "{context}");
    assert_eq!(
        value("answered-awake"),
        "true",
        "the phone may sleep through a call. {context}"
    );
    assert_eq!(value("ringing-buttons-after"), "false", "{context}");

    assert_eq!(value("ended"), "ended", "{context}");
    assert_eq!(value("ended-status"), "Call ended", "{context}");
    assert_eq!(
        value("ended-view"),
        "0",
        "the call's page outlived the call. {context}"
    );
    assert!(
        value("revoked").contains("\"msg\":\"remove\""),
        "the microphone was not taken back from the call's page: {}. {context}",
        value("revoked")
    );
    assert_eq!(
        value("popped"),
        "pop;",
        "the call page did not go by itself once the call was over. {context}"
    );
    assert_eq!(
        value("after"),
        "|",
        "the ended call was not let go. {context}"
    );
    assert!(
        value("after-mce")
            .trim_end()
            .ends_with("\"value\":\"none\"},{\"type\":\"s\",\"value\":\"normal\"}]"),
        "mce was not told the call is over: {}. {context}",
        value("after-mce")
    );
    assert_eq!(
        value("after-awake"),
        "false",
        "the phone was kept awake after the call. {context}"
    );

    assert_eq!(value("ring-again"), "ringing", "{context}");
    assert_eq!(
        value("pushes-again"),
        "2",
        "a second call, after the first page had gone, brought up no page. {context}"
    );
    assert_eq!(value("rang-out"), "ended", "{context}");
    assert_eq!(value("missed"), "ended|missed", "{context}");
    assert_eq!(
        value("missed-published"),
        "true",
        "a call that rang out left nothing behind. {context}"
    );
    assert_eq!(
        value("missed-category"),
        "x-nemo.call.missed",
        "a missed call is not in the phone's own category for one. {context}"
    );
    assert_eq!(value("missed-body"), "Missed call", "{context}");
    assert_eq!(value("missed-actions"), "default,callBack", "{context}");
    assert_eq!(value("missed-ring"), "false", "{context}");
}
