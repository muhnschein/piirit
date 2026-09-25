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

const PROBE_QML: &str = r#"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Sailfish.WebEngine 1.0
    Item {
        id: probe

        // Silica's `pageStack` as the call page uses it: it pops itself
        // once the call is over. Recorded rather than performed, and in
        // QML rather than as a QObject on the Rust side, since a page
        // loaded by a Loader reads `pageStack` off the file the Loader
        // was declared in.
        property QtObject pageStack: QtObject {
            property string log: ''
            function pop() { log = log + 'pop;' }
        }

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
        property string chats: ''
        // The app's own settings, as a file in the components directory
        // reads them: a route an earlier run left preferred, before the
        // call handling starts.
        function setSetting(dir, name, value) {
            var writer = Qt.createQmlObject('import QtQuick 2.0; import "' + dir + '"; '
                + 'QtObject { function set(n, v) { Settings[n] = v } '
                + 'function get(n) { return \'\' + Settings[n] } }', probe)
            writer.set(name, value)
            return writer.get(name)
        }
        function setting(dir, name) {
            var reader = Qt.createQmlObject('import QtQuick 2.0; import "' + dir + '"; '
                + 'QtObject { function get(n) { return \'\' + Settings[n] } }', probe)
            return reader.get(name)
        }
        function load(url, dir) {
            setSetting(dir, 'callRoute', 'earpiece')
            centerLoader.setSource(url, { stack: stack, enabled: true })
            if (centerLoader.status !== Loader.Ready) { return 'load-failed' }
            // ngfd numbers the event it plays; the route manager lists
            // nothing plugged in.
            find(centerLoader.item, 'feedback').reply = 42
            find(centerLoader.item, 'routes').reply = []
            centerLoader.item.chatRequested.connect(function (accountId, chatId) {
                chats += accountId + ':' + chatId + ';'
            })
            return 'ok'
        }
        // A drag on the ringing handset, let go of at dx, dy.
        function swipe(dx, dy) {
            var hit = findIn(pageHolder.item, 'callSwipe')
            if (!hit) { return 'missing:callSwipe' }
            return hit.release(dx, dy)
        }
        function remind() {
            var timer = find(centerLoader.item, 'reminder')
            timer.triggered()
            return 'ok'
        }
        function requested() { return chats }
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
        function popped() { return probe.pageStack.log }
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
"#;

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
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
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
        let center = common::component_url("CallCenter.qml");
        let dir = center.trim_end_matches("CallCenter.qml").to_string();
        record!(
            "load",
            call!("load", QString::from(center), QString::from(dir.clone()))
        );
        // Let go of at once: the earlier run's preference.
        record!("stale-routes", get!("routes", "called"));
        record!(
            "stale-setting",
            call!("setting", QString::from(dir), QString::from("callRoute"))
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
        record!("ringing-buttons", get!("ringingView", "visible"));
        record!("ring-name", get!("ringName", "text"));
        record!("options-before", get!("ringingOptions", "visible"));
        // Short of the mark: nothing.
        record!("short", call!("swipe", 20.0_f64, -20.0_f64));
        // Up: silenced, and what comes instead is offered.
        record!("silence", call!("swipe", 0.0_f64, -400.0_f64));
        record!("silenced-feedback", get!("feedback", "called"));
        record!("options-after", get!("ringingOptions", "visible"));
        record!("silenced-state", call!("callState"));
        record!("hang-up-button", get!("hangUpButton", "visible"));
        record!("ring-published", get!("ringNote", "isPublished"));
        record!("ring-urgency", get!("ringNote", "urgency"));
        record!("ring-body", get!("ringNote", "body"));
        record!("ring-actions", call!("actions", QString::from("ringNote")));
        record!("ringing-feedback", get!("feedback", "called"));
        record!("ringing-mce", get!("mce", "called"));
        record!("ringing-awake", get!("callKeepAlive", "enabled"));
        record!("ringing-lit", get!("callDisplay", "preventBlanking"));
        record!("clock", get!("callClock", "text"));
        record!("mute-ringing", get!("muteSwitch", "visible"));
        // Across: answered.
        record!("answer", call!("swipe", 400.0_f64, 0.0_f64));
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
        record!("ringing-buttons-after", get!("ringingView", "visible"));
        record!("hang-up-answered", get!("hangUpButton", "visible"));
        record!("mute-answered", get!("muteSwitch", "visible"));
        record!("mute-tap", call!("tap", QString::from("muteSwitch")));
        record!("muted", get!("muteSwitch", "checked"));
        // On the earpiece, with nothing plugged in.
        record!("earpiece", get!("routes", "called"));
        record!("speaker-tap", call!("tap", QString::from("speakerSwitch")));
        record!("speaker", get!("speakerSwitch", "checked"));
        record!("loudspeaker", get!("routes", "called"));
        // Unseen, and under a shield: a stray tap reaches none of the
        // call page's own controls.
        record!("view-opacity", get!("callViewLoader", "opacity"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("ended", call!("endedThere"));
        record!("ended-status", get!("callStatus", "text"));
        record!("ended-view", get!("callViewLoader", "status"));
        record!("revoked", call!("engine"));
    });

    // Said for a moment, then gone.
    single_shot(Duration::from_secs(7), move || unsafe {
        record!("popped", call!("popped"));
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
        call!("dropPage");
    });

    // A third call, silenced and declined with a message instead.
    // Straight after the last page was taken away, while it may still be
    // on its way out: the call is shown once it has gone.
    single_shot(Duration::from_millis(10_100), move || unsafe {
        record!("ring-message", call!("ring", 9200));
    });

    single_shot(Duration::from_millis(11_500), move || unsafe {
        record!("message-silence", call!("swipe", 0.0_f64, -400.0_f64));
        record!("message-tap", call!("tap", QString::from("messageOption")));
        record!("message-state", call!("callState"));
    });

    single_shot(Duration::from_millis(12_500), move || unsafe {
        record!("message-chat", call!("requested"));
        call!("dropPage");
    });

    // And a fourth, declined with a reminder to call back.
    single_shot(Duration::from_secs(13), move || unsafe {
        record!("ring-remind", call!("ring", 9300));
    });

    single_shot(Duration::from_secs(14), move || unsafe {
        record!("remind-silence", call!("swipe", 0.0_f64, -400.0_f64));
        record!("remind-tap", call!("tap", QString::from("remindOption")));
        record!("remind-state", call!("callState"));
        record!("remind-pending", get!("reminder", "running"));
        record!("remind-early", get!("remindNote", "isPublished"));
        call!("remind");
        record!("remind-note", get!("remindNote", "isPublished"));
        record!("remind-body", get!("remindNote", "body"));
        record!(
            "remind-actions",
            call!("actions", QString::from("remindNote"))
        );
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
    assert!(
        value("stale-routes").contains(
            "Prefer [{\"type\":\"s\",\"value\":\"earpiece\"},{\"type\":\"u\",\"value\":1},{\"type\":\"u\",\"value\":0}]"
        ),
        "a route an earlier run left preferred was not let go of: {}. {context}",
        value("stale-routes")
    );
    assert_eq!(value("stale-setting"), "", "{context}");
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
    assert!(!value("ring-name").is_empty(), "{context}");
    assert_eq!(value("options-before"), "false", "{context}");
    assert_eq!(value("short"), "back", "{context}");
    assert_eq!(value("silence"), "silenced", "{context}");
    assert!(
        value("silenced-feedback").contains("Stop [{\"type\":\"u\",\"value\":42}]"),
        "swiping up did not stop the ringtone: {}. {context}",
        value("silenced-feedback")
    );
    assert_eq!(
        value("options-after"),
        "true",
        "a silenced call does not offer what to do instead. {context}"
    );
    assert!(
        value("silenced-state").starts_with("ringing|"),
        "silencing a call did more than silence it: {}. {context}",
        value("silenced-state")
    );
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
    assert_eq!(value("ring-body"), "\u{1f4de} Incoming call", "{context}");
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
    assert!(
        value("clock").len() == 5 && value("clock").as_bytes()[2] == b':',
        "the time of day is not on the call screen: {}. {context}",
        value("clock")
    );
    assert_eq!(
        value("mute-ringing"),
        "false",
        "a call not yet answered offers a microphone to switch. {context}"
    );
    assert_eq!(value("answer"), "answered", "{context}");

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
    assert_eq!(value("hang-up-answered"), "true", "{context}");
    assert_eq!(value("mute-answered"), "true", "{context}");
    assert_eq!(value("mute-tap"), "ok", "{context}");
    assert_eq!(
        value("muted"),
        "true",
        "the microphone switch did not switch the call's microphone off. {context}"
    );
    let prefer = |name: &str, set: u8| {
        format!(
            "Prefer [{{\"type\":\"s\",\"value\":\"{name}\"}},{{\"type\":\"u\",\"value\":1}},{{\"type\":\"u\",\"value\":{set}}}]"
        )
    };
    assert!(
        value("earpiece").contains("Routes []")
            && value("earpiece").contains(&prefer("earpiece", 1)),
        "a call with nothing plugged in was not put on the earpiece: {}. {context}",
        value("earpiece")
    );
    assert_eq!(value("speaker-tap"), "ok", "{context}");
    assert_eq!(value("speaker"), "true", "{context}");
    assert!(
        value("loudspeaker")
            .trim_end()
            .ends_with(&prefer("earpiece", 0)),
        "the loudspeaker switch did not let go of the earpiece: {}. {context}",
        value("loudspeaker")
    );
    assert_eq!(
        value("view-opacity"),
        "0",
        "the call's own page, controls and all, is drawn over the call screen. {context}"
    );

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
    assert_eq!(value("missed-body"), "\u{1f4de} Missed call", "{context}");
    assert_eq!(value("missed-actions"), "default,callBack", "{context}");
    assert_eq!(value("missed-ring"), "false", "{context}");

    assert_eq!(value("ring-message"), "ringing", "{context}");
    assert_eq!(value("message-silence"), "silenced", "{context}");
    assert_eq!(value("message-tap"), "ok", "{context}");
    assert_eq!(
        value("message-state"),
        "|",
        "declining with a message did not decline and let the call go. {context}"
    );
    assert_eq!(
        value("message-chat"),
        "1:1;",
        "declining with a message did not open the caller's chat. {context}"
    );

    assert_eq!(value("ring-remind"), "ringing", "{context}");
    assert_eq!(value("remind-silence"), "silenced", "{context}");
    assert_eq!(value("remind-tap"), "ok", "{context}");
    assert_eq!(value("remind-state"), "ended|declined", "{context}");
    assert_eq!(
        value("remind-pending"),
        "true",
        "declining with a reminder set no reminder. {context}"
    );
    assert_eq!(value("remind-early"), "false", "{context}");
    assert_eq!(value("remind-note"), "true", "{context}");
    assert_eq!(value("remind-body"), "\u{1f4de} Call back", "{context}");
    assert_eq!(value("remind-actions"), "default,callBack", "{context}");
}
