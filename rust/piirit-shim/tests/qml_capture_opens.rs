//! The camera page opens the way it is asked to: in video rather than a
//! still, on the front camera rather than the back, and -- for a caller
//! that puts its next page in the camera's place -- without taking itself
//! down once it has answered. A camera quick action on the cover asks for
//! all three (`ChatListPage.capture`). While a video records, the page
//! says that a quick action must not take the stack out from under it.
//!
//! Driven through the `QtMultimedia` stubs, as `qml_capture.rs` is: two
//! cameras on the back and one on the front.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        property string picked: ''
        // Silica's `pageStack`, counted rather than performed.
        property QtObject pageStack: QtObject {
            property int pops: 0
            function pop() { pops += 1 }
        }
        function pops() { return '' + pageStack.pops }
        Loader { id: loader }
        function load(url, properties) {
            loader.setSource('', {})
            picked = ''
            loader.setSource(url, JSON.parse(properties))
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.picked.connect(function(path) { picked = path })
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
            return null
        }
        function camera() { return findIn(loader.item, 'camera') }
        // What the camera was started in: its mode, the device, and
        // whether it runs.
        function opened() {
            var c = camera()
            return c.captureMode + '|' + c.deviceId + '|' + c.running
        }
        function page(property) { return '' + loader.item[property] }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function stillWritten() { camera().imageCapture.saved(); return 'ok' }
        function videoFinished() { camera().videoRecorder.finish(); return 'ok' }
        function pickedPath() { return picked }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_camera_opens_in_the_mode_and_on_the_side_it_is_asked_for() {
    let temp = std::env::temp_dir().join(format!("piirit-capture-opens-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp dir");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("XDG_CACHE_HOME", &temp);
    }

    piirit_shim::register_qml_types();

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

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
    macro_rules! load {
        ($properties:expr) => {
            call!(
                "load",
                QString::from(common::page_url("CapturePage.qml")),
                QString::from($properties)
            )
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        // As the conversation opens it: a still, on the back camera.
        record!("plain", load!("{}"));
        record!("plain-opened", call!("opened"));
        record!("plain-facing", call!("page", QString::from("facing")));

        // A video on the back camera, as the Video action asks.
        record!("video", load!(r#"{"mode": 1, "closeOnPick": false}"#));
        record!("video-opened", call!("opened"));
        record!("video-mode", call!("page", QString::from("mode")));
        record!(
            "idle-paused",
            call!("page", QString::from("pausesQuickActions"))
        );
        record!("record", call!("tap", QString::from("shutter")));
        record!(
            "recording-paused",
            call!("page", QString::from("pausesQuickActions"))
        );
        record!("stop", call!("tap", QString::from("shutter")));
        record!(
            "stopping-paused",
            call!("page", QString::from("pausesQuickActions"))
        );
        record!("pops-before-video", call!("pops"));
        record!("finished", call!("videoFinished"));
        record!("video-picked", call!("pickedPath"));
        record!("pops-after-video", call!("pops"));
        record!(
            "done-paused",
            call!("page", QString::from("pausesQuickActions"))
        );

        // A still on the front camera, as Selfie asks.
        record!("selfie", load!(r#"{"front": true, "closeOnPick": false}"#));
        record!("selfie-opened", call!("opened"));
        record!("selfie-facing", call!("page", QString::from("facing")));
        // The switch goes back to the back camera from there.
        record!("switch", call!("tap", QString::from("flipButton")));
        record!("switched-opened", call!("opened"));
        record!("pops-before-still", call!("pops"));
        record!("shutter", call!("tap", QString::from("shutter")));
        record!("written", call!("stillWritten"));
        record!("still-picked", call!("pickedPath"));
        record!("pops-after-still", call!("pops"));

        // A video on the front camera, as Selfie video asks.
        record!(
            "selfie-video",
            load!(r#"{"mode": 1, "front": true, "closeOnPick": false}"#)
        );
        record!("selfie-video-opened", call!("opened"));

        // Left at its default, the page still takes itself down.
        record!("closing", load!("{}"));
        record!("pops-before-close", call!("pops"));
        record!("close-shutter", call!("tap", QString::from("shutter")));
        record!("close-written", call!("stillWritten"));
        record!("pops-after-close", call!("pops"));
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

    for step in ["plain", "video", "selfie", "selfie-video", "closing"] {
        assert_eq!(
            value(step),
            "ok",
            "the page did not load for {step}. {context}"
        );
    }
    // QCamera's own values: a still 1, a video 2. The stub's Camera.FrontFace
    // reads as 2, as Viewfinder.js names it.
    assert_eq!(
        value("plain-opened"),
        "1|back-0|true",
        "the camera no longer opens on a still with the back camera by \
         default. {context}"
    );
    assert_eq!(
        value("video-opened"),
        "2|back-0|true",
        "asked for a video, the camera did not open recording one. {context}"
    );
    assert_eq!(
        value("video-mode"),
        "1",
        "the mode stack does not say video. {context}"
    );
    assert_eq!(
        value("selfie-opened"),
        "1|front-0|true",
        "asked for the front camera, the page did not open on it. {context}"
    );
    assert_eq!(value("plain-facing"), "1", "{context}");
    assert_eq!(
        value("selfie-facing"),
        "2",
        "the page does not know it is on the front camera. {context}"
    );
    assert_eq!(
        value("switched-opened"),
        "1|back-0|true",
        "the switch from a selfie does not go to the back camera. {context}"
    );
    assert_eq!(
        value("selfie-video-opened"),
        "2|front-0|true",
        "a selfie video did not open recording on the front camera. {context}"
    );

    assert_eq!(
        (
            value("idle-paused").as_str(),
            value("recording-paused").as_str(),
            value("stopping-paused").as_str(),
            value("done-paused").as_str()
        ),
        ("false", "true", "true", "false"),
        "the page does not hold quick actions off while, and only while, a \
         video records or is being finished. {context}"
    );

    assert!(
        value("video-picked").contains("/captures/video-"),
        "the video was not reported. {context}"
    );
    assert!(
        value("still-picked").contains("/captures/photo-"),
        "the still was not reported. {context}"
    );
    assert_eq!(
        value("pops-after-video"),
        value("pops-before-video"),
        "asked not to, the page took itself down after a video. {context}"
    );
    assert_eq!(
        value("pops-after-still"),
        value("pops-before-still"),
        "asked not to, the page took itself down after a still. {context}"
    );
    let before: u32 = value("pops-before-close").parse().unwrap_or_default();
    assert_eq!(
        value("pops-after-close"),
        (before + 1).to_string(),
        "by default the page no longer takes itself down once it has \
         answered. {context}"
    );
}
