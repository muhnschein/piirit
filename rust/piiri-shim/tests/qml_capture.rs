//! The camera page: a still is reported once it is written, a video once
//! the recorder has finished with it, and the page takes itself down
//! with either.
//!
//! Driven through the `QtMultimedia` stubs, which record what the page
//! asked for and let the test answer the way the backend does: a still
//! lands where it was asked to go, a video is finalised after it is
//! stopped and only then has a location -- and a recorder that ignores
//! its stop, which a phone's did, is finished by stopping the camera.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see piiri-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use qmetaobject::*;

mod common;

/// Silica's `pageStack`, recorded rather than performed: the page pops
/// itself once it has answered.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
    pops: qt_property!(i32; NOTIFY pops_changed),
    pops_changed: qt_signal!(),
    pop: qt_method!(fn(&mut self)),
}

impl StackProbe {
    fn pop(&mut self) {
        self.pops += 1;
        self.pops_changed();
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        property string picked: ''
        Loader { id: loader }
        function load(url) {
            loader.setSource('', {})
            picked = ''
            loader.setSource(url, {})
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
        function running() { return '' + camera().running }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // A tap on a MouseArea: its clicked() carries the event, and
        // QML refuses to emit it without one.
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked(null)
            return 'ok'
        }
        // A tap on an IconButton, whose clicked() carries nothing.
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return '' + loader.item.mode
        }
        // What the page asked the camera for.
        function requestedStill() { return camera().imageCapture.requested }
        function requestedVideo() { return '' + camera().videoRecorder.outputLocation }
        // The backend's answers.
        function stillWritten() { camera().imageCapture.saved(); return 'ok' }
        function videoFinished() { camera().videoRecorder.finish(); return 'ok' }
        function ignoreStops() { camera().videoRecorder.stopsOnRequest = false; return 'ok' }
        function recorderState() { return '' + camera().videoRecorder.recorderState }
        function pickedPath() { return picked }
        function shutterIcon() { return loader.item.shutterIcon }
        function leave() { loader.item.status = PageStatus.Deactivating; return 'ok' }
        // The phone is turned: the sensor reports it, and the page writes
        // the turn into the next capture.
        function turn(orientation) {
            findIn(loader.item, 'orientationSensor').report(orientation)
            return '' + camera().metaData.orientation
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_still_and_a_video_are_reported_once_written_and_the_page_leaves() {
    let temp = std::env::temp_dir().join(format!("piiri-capture-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp dir");

    // SAFETY: single-threaded test binary; set before Qt starts. The
    // captures go under the cache directory, pointed at this test's own.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("XDG_CACHE_HOME", &temp);
    }

    piiri_shim::register_qml_types();

    let stack_box = QObjectBox::new(StackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

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
    macro_rules! probe {
        ($label:expr, $name:expr, $property:expr) => {
            record!(
                $label,
                call!("get", QString::from($name), QString::from($property))
            )
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::page_url("CapturePage.qml")))
        );
        // A page loaded on its own is the one on screen, so the camera
        // runs from the start.
        record!("running", call!("running"));
        probe!("mode", "modeColumn", "enabled");
        record!("still-face", call!("shutterIcon"));
        // Held upright, the sensor's landscape frame needs a quarter turn;
        // on its left side, none; on its right, a half. The stub's sensor
        // is mounted at 90 degrees, as a phone's back camera is.
        record!("turn-upright", call!("turn", 1));
        record!("turn-left", call!("turn", 3));
        record!("turn-right", call!("turn", 4));
        record!("turn-upside-down", call!("turn", 2));
        // Face down says nothing about the turn: the last one stays.
        record!("turn-face-down", call!("turn", 6));
        record!("shutter", call!("tap", QString::from("shutter")));
        record!("still-asked", call!("requestedStill"));
        // Nothing reported until the file is on disk.
        record!("still-early", call!("pickedPath"));
        record!("still-written", call!("stillWritten"));
        record!("still-picked", call!("pickedPath"));
        record!(
            "still-pops",
            (*stack_ptr).pinned().borrow().pops.to_string()
        );
        record!("still-stopped", call!("running"));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "reload",
            call!("load", QString::from(common::page_url("CapturePage.qml")))
        );
        // The mode is switched from the stacked buttons, and the camera
        // is running again afterwards.
        record!("video-mode", call!("tap", QString::from("modeOption1")));
        record!("running-in-video-mode", call!("running"));
        record!("record", call!("tap", QString::from("shutter")));
        // What the page shows is the recorder's state, not its own.
        record!("recording", call!("shutterIcon"));
        probe!("time-shown", "recordingIndicator", "visible");
        record!("video-asked", call!("requestedVideo"));
        // The mode cannot be switched under a running recording.
        probe!("mode-locked", "modeColumn", "enabled");
        record!("stop", call!("tap", QString::from("shutter")));
        record!("stopped-state", call!("recorderState"));
        // Stopped is not finished: nothing is reported yet.
        record!("video-early", call!("pickedPath"));
        record!("video-finished", call!("videoFinished"));
        record!("video-picked", call!("pickedPath"));
        record!(
            "video-pops",
            (*stack_ptr).pinned().borrow().pops.to_string()
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        // A recorder that does not act on its stop: the page stops the
        // camera a moment later, which finishes the file.
        record!(
            "reload-stubborn",
            call!("load", QString::from(common::page_url("CapturePage.qml")))
        );
        record!("ignore-stops", call!("ignoreStops"));
        record!("stubborn-mode", call!("tap", QString::from("modeOption1")));
        record!("stubborn-record", call!("tap", QString::from("shutter")));
        record!("stubborn-stop", call!("tap", QString::from("shutter")));
        record!("stubborn-state", call!("recorderState"));
        probe!("stubborn-busy", "writing", "running");
        record!("stubborn-early", call!("pickedPath"));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("stubborn-picked", call!("pickedPath"));
        record!(
            "stubborn-pops",
            (*stack_ptr).pinned().borrow().pops.to_string()
        );
        // Leaving the page stops the camera.
        record!(
            "reload-again",
            call!("load", QString::from(common::page_url("CapturePage.qml")))
        );
        record!("before-leaving", call!("running"));
        call!("leave");
        record!("after-leaving", call!("running"));
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
        "the camera page did not load. {context}"
    );
    assert_eq!(
        value("running"),
        "true",
        "the camera is not running on the page that shows it. {context}"
    );
    assert_eq!(
        value("mode"),
        "true",
        "the mode switch is locked. {context}"
    );
    assert_eq!(
        [
            value("turn-upright"),
            value("turn-left"),
            value("turn-right"),
            value("turn-upside-down"),
            value("turn-face-down"),
        ],
        ["90", "0", "180", "270", "270"],
        "the turn written into a capture does not follow the phone. {context}"
    );
    // What the page asks for is the exact name, extension and all: it is
    // what the other end sees the file called.
    let still = value("still-asked");
    assert!(
        still.contains("/captures/photo-") && still.rsplit('.').next() == Some("jpg"),
        "the still was not asked for in the captures directory as a photo: {still:?}. {context}"
    );
    assert_eq!(
        value("still-early"),
        "",
        "the still was reported before it was written. {context}"
    );
    assert_eq!(
        value("still-picked"),
        still,
        "the still written was not the one reported. {context}"
    );
    assert_eq!(
        value("still-pops"),
        "1",
        "the page did not leave once it had a picture to report. {context}"
    );
    assert_eq!(
        value("still-stopped"),
        "false",
        "the camera kept running after the picture was taken. {context}"
    );

    assert_eq!(
        value("video-mode"),
        "1",
        "the video button did not switch the mode. {context}"
    );
    assert_eq!(
        value("running-in-video-mode"),
        "true",
        "the camera was left stopped after the switch to video. {context}"
    );
    let video = value("video-asked");
    assert!(
        video.starts_with("file://")
            && video.contains("/captures/video-")
            && video.rsplit('.').next() == Some("mp4"),
        "the video was not asked for in the captures directory as a video: {video:?}. {context}"
    );
    assert!(
        value("still-face").ends_with("icon-camera-shutter"),
        "the shutter does not wear the platform's own face for a picture. {context}"
    );
    assert!(
        value("recording").ends_with("icon-camera-video-shutter-off"),
        "the shutter does not show a stop while a video records. {context}"
    );
    assert_eq!(
        value("time-shown"),
        "true",
        "the time does not run while a video records. {context}"
    );
    assert_eq!(
        value("mode-locked"),
        "false",
        "the mode can be switched under a running recording. {context}"
    );
    assert_eq!(
        value("stopped-state"),
        "0",
        "a second tap did not stop the recording. {context}"
    );
    assert_eq!(
        value("video-early"),
        "",
        "the video was reported while the recorder was still finishing it. {context}"
    );
    assert_eq!(
        value("video-picked"),
        video.trim_start_matches("file://"),
        "the video reported is not where the recorder put it. {context}"
    );
    assert_eq!(
        value("video-pops"),
        "2",
        "the page did not leave once it had a video to report. {context}"
    );

    assert_eq!(
        value("stubborn-state"),
        "1",
        "the stub acted on the stop it was told to ignore. {context}"
    );
    assert_eq!(
        value("stubborn-busy"),
        "true",
        "nothing shows that the stop is being waited on. {context}"
    );
    assert_eq!(
        value("stubborn-early"),
        "",
        "a recording the recorder had not finished was reported. {context}"
    );
    assert!(
        value("stubborn-picked").contains("/captures/video-"),
        "a stop the recorder ignored was not made good by stopping the camera. {context}"
    );
    assert_eq!(
        value("stubborn-pops"),
        "3",
        "the page did not leave with the video the camera's stop finished. {context}"
    );
    assert_eq!(value("before-leaving"), "true", "{context}");
    assert_eq!(
        value("after-leaving"),
        "false",
        "the camera kept running after the page was left. {context}"
    );
}
