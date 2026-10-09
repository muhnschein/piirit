//! A video call on its page: rung, answered with the camera or without
//! it, and the pictures shown under the page's own lines and switches.
//!
//! The window's `CallCenter.qml` and the call page, with the platform
//! stubbed as `qml_call_page.rs` stubs it. The page the media runs in is
//! not run; its side of the conversation -- what it reports about the
//! pictures -- is played over a real socket, as the bridge plays it. What
//! is checked is what the reader would see: the camera switch where a
//! video call can be answered without the camera, the pictures shown
//! only while there are any, the lines over them put away by a tap, the
//! screen kept lit and the ear shield kept off while they are up.

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

use common::http::{api_of, authority_of, body_of, get, post, status_of};

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Sailfish.WebEngine 1.0
    Item {
        id: probe

        property QtObject pageStack: QtObject {
            function pop() {}
        }

        Loader { id: centerLoader }
        Loader { id: pageHolder; width: 540; height: 960 }
        QtObject {
            id: stack
            property bool busy: false
            function push(url, props) {
                pageHolder.setSource(url, props)
                return pageHolder.item
            }
        }
        function load(url) {
            // In front, as the app is when the reader is in a call.
            centerLoader.setSource(url, { stack: stack, enabled: true, away: false })
            return centerLoader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function front() {
            if (!pageHolder.item) { return 'no-page' }
            pageHolder.item.appActive = true
            return 'ok'
        }
        // A tap on the pictures, as the shield over them passes it on.
        function tapPictures() { pageHolder.item.toggleControls(); return 'ok' }
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
        function engine() { return WebEngine.notified }
        function call() {
            var call = centerLoader.item.call
            return call.state + ' camera=' + call.camera + ' rear=' + call.rear_camera
        }
        function url() { return centerLoader.item.call.url }
        function ring(messageId, video) {
            centerLoader.item.call.handle_event(1, 'IncomingCall', JSON.stringify({
                kind: 'IncomingCall', msg_id: messageId, chat_id: 1,
                place_call_info: 'v=0 other', has_video: video
            }))
            return centerLoader.item.call.state
        }
        function place(video) {
            return centerLoader.item.place(1, 1, video) ? 'placed' : 'refused'
        }
        function hangUp() { centerLoader.item.call.hang_up(); return 'ok' }
        function dropPage() { pageHolder.setSource('', {}); return 'ok' }
        function reset() { centerLoader.item.call.reset(); return 'ok' }
    }
";

/// Run `steps` one after another: the first `first` from now, each of the
/// rest `gap` after the one before has finished.
fn run_steps(mut steps: Vec<Box<dyn FnMut()>>, first: Duration, gap: Duration) {
    if steps.is_empty() {
        return;
    }
    let mut step = steps.remove(0);
    single_shot(first, move || {
        step();
        run_steps(std::mem::take(&mut steps), gap, gap);
    });
}

fn report(url: &str, route: &str, body: &str) -> String {
    let answer = post(&authority_of(url), &format!("{}{route}", api_of(url)), body);
    status_of(&answer).to_string()
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_video_call_shows_its_pictures_under_lines_a_tap_puts_away() {
    let temp = std::env::temp_dir().join(format!("piirit-call-video-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
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
    let mut page_url = String::new();
    let url_ptr: *mut String = std::ptr::addr_of_mut!(page_url);

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
    macro_rules! tap {
        ($name:expr) => {
            call!("tap", QString::from($name))
        };
    }

    let script: Vec<Box<dyn FnMut()>> = vec![
        // SAFETY for every step: these run only while `exec()` runs on
        // this thread, and everything they point at outlives it.
        Box::new(move || unsafe {
            record!(
                "load",
                call!(
                    "load",
                    QString::from(common::component_url("CallCenter.qml"))
                )
            );
            record!("ring", call!("ring", 9200, true));
        }),
        // A video call rings: said so, and the camera can be switched off
        // before it is answered.
        Box::new(move || unsafe {
            record!("status", get!("callStatus", "text"));
            record!("ring-body", get!("ringNote", "body"));
            record!("switches-ringing", get!("callSwitches", "visible"));
            record!("mute-ringing", get!("muteSwitch", "visible"));
            record!("camera-ringing", get!("cameraSwitch", "checked"));
            record!("flip-ringing", get!("flipButton", "visible"));
            record!("front", call!("front"));
            record!("camera-tap", tap!("cameraSwitch"));
            record!("camera-off-ringing", get!("cameraSwitch", "checked"));
            record!("flip-off-ringing", get!("flipButton", "visible"));
            tap!("cameraSwitch");
            record!("camera-back", call!("call"));
            record!("answer", tap!("answerButton"));
        }),
        // Answered with the camera: the page is granted the camera too,
        // and says the camera is live.
        Box::new(move || unsafe {
            let url = call!("url");
            *url_ptr = url.clone();
            record!("url", url.clone());
            record!("engine", call!("engine"));
            record!("hidden-before", get!("callViewLoader", "opacity"));
            record!(
                "report",
                report(
                    &url,
                    "/video",
                    r#"{"local":true,"remote":false,"front":true}"#
                )
            );
        }),
        Box::new(move || unsafe {
            let url = (*url_ptr).clone();
            record!("shown", get!("callViewLoader", "opacity"));
            record!("lit", get!("callDisplay", "preventBlanking"));
            record!("ear", get!("proximity", "active"));
            record!("top-shade", get!("topShade", "visible"));
            record!("foot-shade", get!("footShade", "visible"));
            record!("mute-video", get!("muteSwitch", "visible"));
            // Their picture comes, and the two ends hear each other.
            report(&url, "/state", "connected");
            report(
                &url,
                "/video",
                r#"{"local":true,"remote":true,"front":true}"#,
            );
        }),
        Box::new(move || unsafe {
            record!("clock", get!("callClock", "text"));
            record!("status-video", get!("callStatus", "visible"));
            record!("tuck", get!("tuck", "running"));
            // A tap puts the lines away, and another brings them back.
            record!("tap-away", call!("tapPictures"));
            record!("who-away", get!("callWho", "visible"));
            record!("hang-up-away", get!("hangUpButton", "visible"));
            record!("switches-away", get!("callSwitches", "visible"));
            record!("shade-away", get!("topShade", "visible"));
            call!("tapPictures");
            record!("who-back", get!("callWho", "visible"));
            record!("hang-up-back", get!("hangUpButton", "visible"));
            // Turned, and switched off.
            record!("flip", tap!("flipButton"));
            record!("camera-off", tap!("cameraSwitch"));
            record!("after-switches", call!("call"));
            let url = (*url_ptr).clone();
            record!(
                "told",
                body_of(&get(
                    &authority_of(&url),
                    &format!("{}/commands", api_of(&url))
                ))
                .to_string()
            );
            // No pictures any more: the page is unseen again.
            report(
                &url,
                "/video",
                r#"{"local":false,"remote":false,"front":false}"#,
            );
        }),
        Box::new(move || unsafe {
            record!("unseen", get!("callViewLoader", "opacity"));
            record!("unlit", get!("callDisplay", "preventBlanking"));
            record!("shade-gone", get!("topShade", "visible"));
            record!("status-back", get!("callStatus", "visible"));
            record!("who-voice", get!("callWho", "visible"));
            record!("ear-voice", get!("proximity", "active"));
            call!("hangUp");
        }),
        Box::new(move || unsafe {
            call!("dropPage");
            call!("reset");
            // A voice call rings with no camera to switch.
            record!("ring-voice", call!("ring", 9201, false));
        }),
        Box::new(move || unsafe {
            record!("status-voice", get!("callStatus", "text"));
            record!("ring-body-voice", get!("ringNote", "body"));
            record!("switches-voice", get!("callSwitches", "visible"));
            call!("hangUp");
            (*engine_ptr).quit();
        }),
    ];
    // Whole seconds: the vendored `single_shot` drops the fraction.
    run_steps(script, Duration::from_secs(2), Duration::from_secs(1));

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "{context}");
    assert_eq!(value("ring"), "ringing", "{context}");
    assert_eq!(value("status"), "Incoming video call", "{context}");
    assert!(
        value("ring-body").contains("Incoming video call"),
        "the notification does not say it is a video call. {context}"
    );
    assert_eq!(
        value("switches-ringing"),
        "true",
        "a ringing video call has no camera switch. {context}"
    );
    assert_eq!(
        value("mute-ringing"),
        "false",
        "a microphone switch on a call nobody has answered. {context}"
    );
    assert_eq!(value("camera-ringing"), "true", "{context}");
    assert_eq!(value("flip-ringing"), "true", "{context}");
    assert_eq!(value("camera-tap"), "ok", "{context}");
    assert_eq!(
        value("camera-off-ringing"),
        "false",
        "the camera could not be switched off before answering. {context}"
    );
    assert_eq!(
        value("flip-off-ringing"),
        "false",
        "a camera that is off can still be turned. {context}"
    );
    assert_eq!(
        value("camera-back"),
        "ringing camera=true rear=false",
        "{context}"
    );
    assert_eq!(value("answer"), "ok", "{context}");

    let url = value("url");
    assert!(
        url.contains("/index.html?&key=") && url.contains("#acceptCall="),
        "the call was not answered with the camera: {url}. {context}"
    );
    let granted = value("engine");
    assert!(
        granted.contains(r#""type":"camera""#) && granted.contains(r#""type":"microphone""#),
        "the call's page was not granted the camera and the microphone: {granted}. {context}"
    );
    assert_eq!(
        value("hidden-before"),
        "0",
        "the page is shown before it has any pictures. {context}"
    );
    assert_eq!(value("report"), "HTTP/1.1 204 No Content", "{context}");

    assert_eq!(
        value("shown"),
        "1",
        "the camera's picture is not shown. {context}"
    );
    assert_eq!(
        value("lit"),
        "true",
        "the screen may go dark in the middle of a video call. {context}"
    );
    assert_eq!(
        value("ear"),
        "false",
        "a video call blacks the screen out when a hand covers the sensor. {context}"
    );
    assert_eq!(value("top-shade"), "true", "{context}");
    assert_eq!(value("foot-shade"), "true", "{context}");
    assert_eq!(value("mute-video"), "true", "{context}");

    let clock = value("clock");
    assert!(
        clock.len() == 8 && clock.chars().filter(|c| *c == ':').count() == 2,
        "the corner does not count the call over pictures: {clock}. {context}"
    );
    assert_eq!(
        value("status-video"),
        "false",
        "the large clock is drawn over a face. {context}"
    );
    assert_eq!(
        value("tuck"),
        "true",
        "the lines never go by themselves. {context}"
    );
    assert_eq!(value("tap-away"), "ok", "{context}");
    assert_eq!(
        value("who-away"),
        "false",
        "a tap did not put the lines away. {context}"
    );
    assert_eq!(value("hang-up-away"), "false", "{context}");
    assert_eq!(value("switches-away"), "false", "{context}");
    assert_eq!(value("shade-away"), "false", "{context}");
    assert_eq!(
        value("who-back"),
        "true",
        "a tap did not bring the lines back. {context}"
    );
    assert_eq!(value("hang-up-back"), "true", "{context}");
    assert_eq!(value("flip"), "ok", "{context}");
    assert_eq!(value("camera-off"), "ok", "{context}");
    assert_eq!(
        value("after-switches"),
        "connected camera=false rear=true",
        "the switches did not reach the call. {context}"
    );
    assert_eq!(
        value("told"),
        r#"["facing-environment","camera-off"]"#,
        "the page was not told what the switches did. {context}"
    );

    assert_eq!(
        value("unseen"),
        "0",
        "the page is still shown with no pictures. {context}"
    );
    assert_eq!(value("unlit"), "false", "{context}");
    assert_eq!(value("shade-gone"), "false", "{context}");
    assert_eq!(value("status-back"), "true", "{context}");
    assert_eq!(value("who-voice"), "true", "{context}");
    assert_eq!(
        value("ear-voice"),
        "true",
        "the ear shield is off in a voice call held to the ear. {context}"
    );

    assert_eq!(value("ring-voice"), "ringing", "{context}");
    assert_eq!(value("status-voice"), "Incoming call", "{context}");
    assert!(
        !value("ring-body-voice").contains("video"),
        "a voice call is announced as a video call. {context}"
    );
    assert_eq!(
        value("switches-voice"),
        "false",
        "a ringing voice call shows switches. {context}"
    );
}
