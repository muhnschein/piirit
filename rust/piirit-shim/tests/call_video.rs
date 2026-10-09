//! Video in a call: the camera as the call screen switches it, and the
//! pictures as the page reports them.
//!
//! The camera is the page's to open and the bridge's to switch
//! (`call_video.js`, tested in Node). What runs here is the `Call` a
//! window holds and the host between it and the page: which way the page
//! is loaded -- camera on or off -- what it is told when the reader
//! switches the camera, turns it or leaves the app, and what becomes of
//! what it reports. The page's side is played over a real socket, as the
//! bridge plays it.

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
    import Piirit 1.0
    Item {
        property int videoChanges: 0
        Call {
            id: call
            onVideo_changed: videoChanges += 1
        }
        Connections {
            target: core
            onCore_event: call.handle_event(context_id, kind, payload_json)
        }
        function video() {
            return 'camera=' + call.camera + ' rear=' + call.rear_camera
                   + ' local=' + call.local_video + ' remote=' + call.remote_video
        }
        function place(video) { return call.place(1, 1, video) ? 'placed' : 'refused' }
        function url() { return call.url }
        function camera(on) { call.set_camera(on); return video() }
        function flip() { call.flip_camera(); return video() }
        function away(on) { call.set_background(on); return video() }
        function hangUp() { call.hang_up(); return call.state + ' ' + video() }
        function reset() { call.reset(); return call.state }
        function answer() { call.answer(); return call.state }
        function changes() { return '' + videoChanges }
        function ring(messageId, hasVideo) {
            call.handle_event(1, 'IncomingCall', JSON.stringify({
                kind: 'IncomingCall', msg_id: messageId, chat_id: 1,
                place_call_info: 'v=0 other-offer', has_video: hasVideo
            }))
            return call.state + ' ' + video()
        }
    }
";

/// The commands waiting for the page, as the bridge collects them.
fn commands(url: &str) -> String {
    let answer = get(&authority_of(url), &format!("{}/commands", api_of(url)));
    body_of(&answer).to_string()
}

/// What the bridge reports on `/video`, answered.
fn report(url: &str, body: &str) -> String {
    let answer = post(&authority_of(url), &format!("{}/video", api_of(url)), body);
    status_of(&answer).to_string()
}

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

#[test]
#[allow(clippy::too_many_lines)]
fn the_camera_is_switched_turned_and_let_go_and_the_pictures_reported() {
    let temp = std::env::temp_dir().join(format!("piirit-call-video-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

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

    // One step after another, each a moment after the last has finished:
    // what a step asked of the host has reached the call before the next
    // one looks.
    let script: Vec<Box<dyn FnMut()>> = vec![
        // SAFETY for every step: these run only while `exec()` runs on
        // this thread, and everything they point at outlives it.
        Box::new(move || unsafe {
            // Nothing to switch without a call.
            record!("idle", call!("camera", true));
            record!("place", call!("place", true));
            record!("placing", call!("video"));
        }),
        // A video call's page starts with its camera on.
        Box::new(move || unsafe {
            let url = call!("url");
            record!("url", url.clone());
            *url_ptr = url.clone();
            record!("before", call!("changes"));
            record!(
                "report",
                report(
                    &url,
                    r#"{"local":true,"remote":false,"front":true,"failed":false}"#
                )
            );
            record!("not-json", report(&url, "camera, please"));
            record!("not-object", report(&url, "[true]"));
        }),
        Box::new(move || unsafe {
            let url = (*url_ptr).clone();
            record!("reported", call!("video"));
            record!("changed", call!("changes"));
            // Off, turned, on: each told to the page as it happens.
            record!("off", call!("camera", false));
            record!("off-told", commands(&url));
            record!("flip", call!("flip"));
            record!("flip-told", commands(&url));
            record!("on", call!("camera", true));
            record!("on-told", commands(&url));
            // Away: the camera let go and the pictures not drawn, and the
            // reader's camera still wanted.
            record!("away", call!("away", true));
            record!("away-told", commands(&url));
            // Switched off and on while away: nothing to tell the page,
            // whose camera is closed either way.
            record!("off-away", call!("camera", false));
            record!("on-away", call!("camera", true));
            record!("back", call!("away", false));
            record!("back-told", commands(&url));
            // What the page reports, taken: their picture, and which way
            // the camera faces.
            record!(
                "theirs",
                report(&url, r#"{"local":true,"remote":true,"front":true}"#)
            );
        }),
        Box::new(move || unsafe {
            let url = (*url_ptr).clone();
            record!("theirs-shown", call!("video"));
            // A camera that would not open.
            report(
                &url,
                r#"{"local":false,"remote":true,"front":true,"failed":true}"#,
            );
        }),
        Box::new(move || unsafe {
            record!("failed", call!("video"));
            record!("hang-up", call!("hangUp"));
            record!("reset", call!("reset"));
            // A voice call's page starts with its camera off.
            record!("place-voice", call!("place", false));
            record!("voice", call!("video"));
        }),
        Box::new(move || unsafe {
            let url = call!("url");
            record!("voice-url", url.clone());
            call!("hangUp");
            call!("reset");
            // A video call rings, its camera on until switched off; turned
            // and sent away before it is answered.
            record!("ring", call!("ring", 9100, true));
            record!("ring-off", call!("camera", false));
            record!("ring-flip", call!("flip"));
            record!("ring-away", call!("away", true));
            record!("answer", call!("answer"));
        }),
        Box::new(move || unsafe {
            let url = call!("url");
            record!("answer-url", url.clone());
            record!("answer-told", commands(&url));
            call!("away", false);
            call!("hangUp");
            call!("reset");
            // A voice call rings with the camera off; answered with it on.
            record!("ring-voice", call!("ring", 9101, false));
            record!("ring-on", call!("camera", true));
            record!("answer-on", call!("answer"));
        }),
        Box::new(move || unsafe {
            record!("answer-on-url", call!("url"));
            call!("hangUp");
            (*engine_ptr).quit();
        }),
    ];
    // The core first has to come up.
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

    assert_eq!(
        value("idle"),
        "camera=false rear=false local=false remote=false",
        "a camera was switched on with no call. {context}"
    );
    assert_eq!(value("place"), "placed", "{context}");
    assert_eq!(
        value("placing"),
        "camera=true rear=false local=false remote=false",
        "a video call does not start with the camera wanted. {context}"
    );
    let url = value("url");
    assert!(
        url.contains("/index.html?&key=") && url.ends_with("#startCall"),
        "a video call's page was not loaded with its camera on: {url}. {context}"
    );
    assert!(
        !url.contains("disableVideoCompletely"),
        "the page would drop the other end's picture: {url}. {context}"
    );
    assert_eq!(value("report"), "HTTP/1.1 204 No Content", "{context}");
    assert_eq!(value("not-json"), "HTTP/1.1 400 Bad Request", "{context}");
    assert_eq!(value("not-object"), "HTTP/1.1 400 Bad Request", "{context}");
    assert_eq!(
        value("reported"),
        "camera=true rear=false local=true remote=false",
        "the page's live camera was not taken. {context}"
    );
    assert!(
        value("changed").parse::<u32>().unwrap_or_default()
            > value("before").parse::<u32>().unwrap_or(u32::MAX),
        "the call did not say its pictures changed. {context}"
    );

    assert_eq!(
        value("off"),
        "camera=false rear=false local=true remote=false",
        "{context}"
    );
    assert_eq!(value("off-told"), r#"["camera-off"]"#, "{context}");
    assert_eq!(
        value("flip"),
        "camera=false rear=true local=true remote=false",
        "{context}"
    );
    assert_eq!(value("flip-told"), r#"["facing-environment"]"#, "{context}");
    assert_eq!(value("on-told"), r#"["camera-on"]"#, "{context}");
    assert_eq!(
        value("away"),
        "camera=true rear=true local=true remote=false",
        "going away switched the reader's camera off. {context}"
    );
    assert_eq!(
        value("away-told"),
        r#"["hidden","camera-off"]"#,
        "the page was not told the app went away. {context}"
    );
    assert_eq!(
        value("off-away"),
        "camera=false rear=true local=true remote=false"
    );
    assert_eq!(
        value("back-told"),
        r#"["shown","camera-on"]"#,
        "coming back did not open the camera again, or told more than that. {context}"
    );

    assert_eq!(value("theirs"), "HTTP/1.1 204 No Content", "{context}");
    assert_eq!(
        value("theirs-shown"),
        "camera=true rear=false local=true remote=true",
        "their picture, or the page's word on which way it faces, was not taken. {context}"
    );
    assert_eq!(
        value("failed"),
        "camera=false rear=false local=false remote=true",
        "a camera that would not open is still shown as on. {context}"
    );
    assert_eq!(
        value("hang-up"),
        "ended camera=false rear=false local=false remote=false",
        "an ended call still has pictures. {context}"
    );

    assert_eq!(
        value("voice"),
        "camera=false rear=false local=false remote=false",
        "{context}"
    );
    let voice_url = value("voice-url");
    assert!(
        voice_url.contains("/index.html?noOutgoingVideoInitially&key="),
        "a voice call's page was loaded with its camera on: {voice_url}. {context}"
    );

    assert_eq!(
        value("ring"),
        "ringing camera=true rear=false local=false remote=false",
        "a ringing video call is not answered with the camera. {context}"
    );
    assert_eq!(
        value("ring-off"),
        "camera=false rear=false local=false remote=false",
        "the camera could not be switched off before answering. {context}"
    );
    assert_eq!(value("answer"), "starting", "{context}");
    let answer_url = value("answer-url");
    assert!(
        answer_url.contains("?noOutgoingVideoInitially&key=")
            && answer_url.contains("#acceptCall="),
        "a video call answered without the camera opened it: {answer_url}. {context}"
    );
    assert_eq!(
        value("answer-told"),
        r#"["facing-environment","hidden"]"#,
        "what was decided before the page was up was not told it. {context}"
    );
    assert_eq!(
        value("ring-voice"),
        "ringing camera=false rear=false local=false remote=false",
        "{context}"
    );
    assert_eq!(
        value("ring-on"),
        "camera=true rear=false local=false remote=false",
        "{context}"
    );
    let answer_on_url = value("answer-on-url");
    assert!(
        answer_on_url.contains("?&key=") && answer_on_url.contains("#acceptCall="),
        "a voice call answered with the camera did not open it: {answer_on_url}. {context}"
    );
}
