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
    import 'file://__COMPONENTS__'
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
            if (centerLoader.status !== Loader.Ready) { return 'load-failed' }
            // The audio policy answers what it is sent at once, and takes
            // it: the next message goes once the last is answered.
            var manager = findIn(centerLoader.item, 'resourceManager')
            if (!manager) { return 'missing:resourceManager' }
            manager.replies = [9, 1, 1, 0, 'OK']
            return 'ok'
        }
        function front() {
            if (!pageHolder.item) { return 'no-page' }
            pageHolder.item.appActive = true
            return 'ok'
        }
        // A tap on the pictures, as the shield over them passes it on.
        function tapPictures() { pageHolder.item.toggleControls(); return 'ok' }
        function turning() { return '' + pageHolder.item.turning }
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
        // Where a spinner is, against the middle of what it is over: to
        // the pixel, which is what centring aligns to.
        function centredOn(name, over) {
            var spinner = findIn(pageHolder.item, name)
            var under = findIn(pageHolder.item, over)
            return Math.abs(spinner.x + spinner.width / 2 - under.x - under.width / 2) <= 1
                    && Math.abs(spinner.y + spinner.height / 2 - under.y - under.height / 2) <= 1
        }
        function engine() { return WebEngine.notified }
        function call() {
            var call = centerLoader.item.call
            return call.state + ' camera=' + call.camera + ' rear=' + call.rear_camera
        }
        function url() { return centerLoader.item.call.url }
        // What the call's sound has had sent to the audio policy since
        // the last look: the methods in order, a register or an update
        // with the resources it asks for.
        function audioSaid() {
            var manager = findIn(centerLoader.item, 'resourceManager')
            if (!manager) { return 'missing:resourceManager' }
            var said = manager.called.trim().split('\n').filter(function (line) {
                return line.length > 0
            }).map(function (line) {
                var method = line.substring(0, line.indexOf(' '))
                var args = JSON.parse(line.substring(line.indexOf(' ') + 1))
                return method === 'register' || method === 'update' ? method + ':' + args[3].value : method
            })
            manager.called = ''
            return said.join(',')
        }
        // The audio policy refuses what it is sent from now on, answering
        // when told to; or no longer does.
        function audioRefuses(on) {
            var manager = findIn(centerLoader.item, 'resourceManager')
            manager.failure = on ? ['org.freedesktop.DBus.Error.AccessDenied', 'no'] : undefined
            manager.deferred = on
            return 'ok'
        }
        // Every message out is answered, and each one sent after it, until
        // nothing is left to send.
        function audioAnswers() {
            var manager = findIn(centerLoader.item, 'resourceManager')
            while (manager.held.length > 0) { manager.answer() }
            return 'ok'
        }
        // A phone whose platform camera lists its wide lens first.
        function lenses() {
            Settings.backLensLabelsConfig.value = ['0.5', '1.0']
            return centerLoader.item.call.back_lenses
        }
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
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    engine.load_data(QByteArray::from(
        PROBE_QML
            .replace("__COMPONENTS__", &components.display().to_string())
            .as_str(),
    ));

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
            record!("lenses", call!("lenses"));
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
            // Nothing about the sound while it rings: the ringtone is the
            // phone's.
            record!("speaker-ringing", get!("speakerSwitch", "visible"));
            record!("audio-ringing", call!("audioSaid"));
            record!("front", call!("front"));
            record!("camera-tap", tap!("cameraSwitch"));
            record!("camera-off-ringing", get!("cameraSwitch", "checked"));
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
            // Answered: the call's sound is held with the policy.
            record!("audio-answered", call!("audioSaid"));
            record!("hidden-before", get!("callViewLoader", "opacity"));
            // No picture yet, but the camera is on: held in front of the
            // face, not to the ear.
            record!("ear-camera", get!("proximity", "active"));
            record!(
                "report",
                report(
                    &url,
                    "/video",
                    r#"{"local":true,"remote":false,"front":true}"#
                )
            );
        }),
        // The camera is open, and its picture still black.
        Box::new(move || unsafe {
            record!("our-spinner-black", get!("ourSpinner", "running"));
            record!("their-spinner-none", get!("theirSpinner", "running"));
            let url = (*url_ptr).clone();
            report(
                &url,
                "/video",
                r#"{"local":true,"remote":false,"localPainted":true,"front":true}"#,
            );
        }),
        Box::new(move || unsafe {
            let url = (*url_ptr).clone();
            record!("our-spinner-shown", get!("ourSpinner", "running"));
            record!("own-picture-whole", get!("ownPicture", "visible"));
            record!("shown", get!("callViewLoader", "opacity"));
            record!("lit", get!("callDisplay", "preventBlanking"));
            record!("ear", get!("proximity", "active"));
            record!("top-shade", get!("topShade", "visible"));
            record!("foot-shade", get!("footShade", "visible"));
            record!("mute-video", get!("muteSwitch", "visible"));
            // Nothing turns the camera while its picture fills the
            // screen: no button in the row, none on the picture.
            record!("flip-row", get!("flipButton", "visible"));
            record!("flip-picture", get!("pictureFlip", "visible"));
            // Their picture comes, black at first, and the two ends hear
            // each other.
            report(&url, "/state", "connected");
            report(
                &url,
                "/video",
                r#"{"local":true,"remote":true,"localPainted":true,"front":true}"#,
            );
        }),
        Box::new(move || unsafe {
            record!("their-spinner-black", get!("theirSpinner", "running"));
            record!("our-spinner-corner", get!("ourSpinner", "running"));
            // Told nothing of where its corner is: the page has it.
            let url = (*url_ptr).clone();
            record!(
                "told-corner",
                body_of(&get(
                    &authority_of(&url),
                    &format!("{}/commands", api_of(&url))
                ))
                .to_string()
            );
            record!("own-picture-corner", get!("ownPicture", "visible"));
            record!(
                "own-picture-place",
                format!(
                    "{},{},{},{}",
                    get!("ownPicture", "x"),
                    get!("ownPicture", "y"),
                    get!("ownPicture", "width"),
                    get!("ownPicture", "height")
                )
            );
            record!("flip-on-picture", get!("pictureFlip", "visible"));
            record!("flip-inner", get!("pictureFlip", "x"));
            record!(
                "flip-size",
                format!(
                    "{},{},{}",
                    get!("pictureFlip", "width"),
                    get!("pictureFlip", "y"),
                    get!("pictureFlipButton", "width")
                )
            );
            // A video call is on the loudspeaker; the switch puts it on
            // the earpiece and back.
            record!(
                "speaker-video",
                format!(
                    "{},{},{}",
                    get!("speakerSwitch", "visible"),
                    get!("speakerSwitch", "enabled"),
                    get!("speakerSwitch", "checked")
                )
            );
            record!("speaker-tap", tap!("speakerSwitch"));
            record!("speaker-earpiece", get!("speakerSwitch", "checked"));
            record!("audio-earpiece", call!("audioSaid"));
            tap!("speakerSwitch");
            record!("audio-speaker", call!("audioSaid"));
            // Refused by the policy: given back, and the switch dimmed.
            call!("audioRefuses", true);
            tap!("speakerSwitch");
            call!("audioAnswers");
            record!("audio-refused", call!("audioSaid"));
            record!("speaker-refused", get!("speakerSwitch", "enabled"));
            call!("audioRefuses", false);
            record!("clock", get!("callClock", "text"));
            record!("status-video", get!("callStatus", "visible"));
            record!("tuck", get!("tuck", "running"));
            // A tap puts the lines away, and another brings them back.
            record!("tap-away", call!("tapPictures"));
            record!("who-away", get!("callWho", "visible"));
            record!("hang-up-away", get!("hangUpButton", "visible"));
            record!("switches-away", get!("callSwitches", "visible"));
            record!("shade-away", get!("topShade", "visible"));
            record!("flip-on-picture-away", get!("pictureFlip", "visible"));
            record!(
                "own-picture-away",
                format!("{},{}", get!("ownPicture", "x"), get!("ownPicture", "y"))
            );
            call!("tapPictures");
            record!("who-back", get!("callWho", "visible"));
            record!("hang-up-back", get!("hangUpButton", "visible"));
        }),
        Box::new(move || unsafe {
            let url = (*url_ptr).clone();
            // Turned on the picture: the button dimmed where it was, from
            // the press on, until the other camera's picture shows.
            record!("turn", tap!("pictureFlipButton"));
            record!("our-spinner-turning", get!("ourSpinner", "running"));
            record!("turning", call!("turning"));
            record!("turn-limit", get!("turnLimit", "running"));
            record!("flip-turning-dim", get!("pictureFlip", "opacity"));
            record!("flip-turning-enabled", get!("pictureFlipButton", "enabled"));
            // Theirs shows; ours, given the other camera, is black again.
            report(
                &url,
                "/video",
                r#"{"local":true,"remote":true,"localPainted":false,"remotePainted":true,"front":false}"#,
            );
        }),
        Box::new(move || unsafe {
            record!("their-spinner-shown", get!("theirSpinner", "running"));
            record!("flip-on-picture-turning", get!("pictureFlip", "visible"));
            record!("turning-black", call!("turning"));
            record!("our-spinner-black-corner", get!("ourSpinner", "running"));
            record!(
                "our-spinner-on-picture",
                call!(
                    "centredOn",
                    QString::from("ourSpinner"),
                    QString::from("ownPicture")
                )
            );
            record!("our-spinner-size", get!("ourSpinner", "size"));
            // The other camera's picture shows: turned.
            let url = (*url_ptr).clone();
            report(
                &url,
                "/video",
                r#"{"local":true,"remote":true,"localPainted":true,"remotePainted":true,"front":false}"#,
            );
        }),
        Box::new(move || unsafe {
            record!("turned", call!("turning"));
            record!("flip-turned-dim", get!("pictureFlip", "opacity"));
            record!("flip-turned-enabled", get!("pictureFlipButton", "enabled"));
            // Black again with no turn: nothing to turn.
            let url = (*url_ptr).clone();
            report(
                &url,
                "/video",
                r#"{"local":true,"remote":true,"localPainted":false,"remotePainted":true,"front":false}"#,
            );
        }),
        Box::new(move || unsafe {
            record!("flip-on-picture-black", get!("pictureFlip", "visible"));
            let url = (*url_ptr).clone();
            report(
                &url,
                "/video",
                r#"{"local":true,"remote":true,"localPainted":true,"remotePainted":true,"front":false}"#,
            );
        }),
        Box::new(move || unsafe {
            // Turned back, and switched off: no turn left to wait for.
            record!("flip", tap!("pictureFlipButton"));
            record!("camera-off", tap!("cameraSwitch"));
            record!("turning-off", call!("turning"));
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
            // Only their picture now.
            report(
                &url,
                "/video",
                r#"{"local":false,"remote":true,"front":false}"#,
            );
        }),
        Box::new(move || unsafe {
            // Their picture is watched with the camera off: still not
            // a call held to the ear.
            record!("ear-watching", get!("proximity", "active"));
            // No pictures any more: the page is unseen again.
            let url = (*url_ptr).clone();
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
            // Over: nothing held, nothing more to give back.
            record!("audio-over", call!("audioSaid"));
            record!("audio-talking-over", get!("callAudio", "talking"));
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
    assert_eq!(value("camera-tap"), "ok", "{context}");
    assert_eq!(
        value("speaker-ringing"),
        "false",
        "the loudspeaker can be switched while the call rings. {context}"
    );
    assert_eq!(
        value("audio-ringing"),
        "",
        "the call's sound was held with the policy while it rang. {context}"
    );
    assert_eq!(
        value("camera-off-ringing"),
        "false",
        "the camera could not be switched off before answering. {context}"
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
    assert_eq!(
        value("audio-answered"),
        "register:3,audio,acquire",
        "a video call's sound was not held with the policy, on the loudspeaker, once answered. {context}"
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
    assert_eq!(
        value("ear-camera"),
        "false",
        "the camera is on, yet a hand over the sensor blacks the screen out. {context}"
    );
    assert_eq!(value("report"), "HTTP/1.1 204 No Content", "{context}");
    assert_eq!(
        value("our-spinner-black"),
        "true",
        "nothing says the black picture is on its way. {context}"
    );
    assert_eq!(value("their-spinner-none"), "false", "{context}");
    assert_eq!(
        value("our-spinner-shown"),
        "false",
        "a spinner over a picture that shows. {context}"
    );
    assert_eq!(
        value("own-picture-whole"),
        "false",
        "the picture filling the screen has a corner of its own. {context}"
    );

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
    assert_eq!(
        value("flip-row"),
        "missing:flipButton",
        "the row has a button that turns the camera. {context}"
    );
    assert_eq!(
        value("flip-picture"),
        "false",
        "the camera can be turned while its picture fills the screen. {context}"
    );

    assert_eq!(
        value("their-spinner-black"),
        "true",
        "nothing says their black picture is on its way. {context}"
    );
    assert_eq!(value("our-spinner-corner"), "false", "{context}");
    assert_eq!(
        value("told-corner"),
        r#"["back-camera-1"]"#,
        "the page was told where the picture's corner is, which it has. {context}"
    );
    assert_eq!(
        value("own-picture-corner"),
        "true",
        "this end's picture in its corner has nothing on it. {context}"
    );
    // The page's own corner: 27% of 540 by 36%, 4% in from the right,
    // 10% of 960 down.
    let place: Vec<f64> = value("own-picture-place")
        .split(',')
        .map(|part| part.parse().unwrap_or(f64::NAN))
        .collect();
    for (got, want) in place.iter().zip([372.6, 96.0, 145.8, 194.4]) {
        assert!(
            (got - want).abs() < 0.01,
            "this end's picture is not where the page draws it: {place:?}. {context}"
        );
    }
    assert_eq!(
        value("flip-on-picture"),
        "true",
        "the picture in its corner has no button to turn the camera. {context}"
    );
    assert_eq!(
        value("flip-inner"),
        "8",
        "the button is not on the picture's inner side. {context}"
    );
    assert_eq!(
        value("flip-size"),
        "48,138.4,64",
        "the button on the picture is not small, a margin up from its foot, \
         with room round it to press. {context}"
    );
    assert_eq!(
        value("flip-on-picture-away"),
        "false",
        "the button stays when the lines go. {context}"
    );
    assert!(
        value("own-picture-place").starts_with(&format!("{},", value("own-picture-away"))),
        "the picture moved when the lines went. {context}"
    );
    assert_eq!(
        value("their-spinner-shown"),
        "false",
        "a spinner over their picture once it shows. {context}"
    );
    assert_eq!(
        value("flip-on-picture-black"),
        "false",
        "the camera can be turned while its picture is black. {context}"
    );
    assert_eq!(value("turn"), "ok", "{context}");
    assert_eq!(
        format!(
            "{} {} {} {}",
            value("turning"),
            value("turn-limit"),
            value("flip-turning-dim"),
            value("flip-turning-enabled")
        ),
        "true true 0.4 false",
        "pressed, the button is not dimmed and held from the press. {context}"
    );
    assert_eq!(
        value("our-spinner-turning"),
        "true",
        "pressed, nothing says the other camera's picture is on its way. {context}"
    );
    assert_eq!(
        format!(
            "{} {}",
            value("flip-on-picture-turning"),
            value("turning-black")
        ),
        "true true",
        "turning, the button left its place while the picture is black. {context}"
    );
    assert_eq!(
        format!(
            "{} {} {}",
            value("turned"),
            value("flip-turned-dim"),
            value("flip-turned-enabled")
        ),
        "false 1 true",
        "the other camera's picture shows, and the button is still held. {context}"
    );
    assert_eq!(
        value("turning-off"),
        "false",
        "switched off, the camera is still waited for. {context}"
    );
    assert_eq!(value("our-spinner-black-corner"), "true", "{context}");
    assert_eq!(
        value("our-spinner-on-picture"),
        "true",
        "the spinner is not over this end's picture in its corner. {context}"
    );
    // BusyIndicatorSize.Medium: the corner is no place for a large one.
    assert_eq!(value("our-spinner-size"), "2", "{context}");

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
        "connected camera=false rear=false",
        "the switches did not reach the call. {context}"
    );
    assert_eq!(
        value("lenses"),
        "0.5,1.0",
        "the call does not have the platform camera's lens labels. {context}"
    );
    assert_eq!(
        value("told"),
        r#"["facing-environment","facing-user","camera-off"]"#,
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
    assert_eq!(
        value("ear-watching"),
        "false",
        "watching their picture with the camera off, a hand over the sensor blacks the screen out. {context}"
    );

    assert_eq!(
        value("speaker-video"),
        "true,true,true",
        "a video call is not on the loudspeaker, or its switch is not there to use. {context}"
    );
    assert_eq!(value("speaker-tap"), "ok", "{context}");
    assert_eq!(value("speaker-earpiece"), "false", "{context}");
    assert_eq!(
        value("audio-earpiece"),
        "update:1,acquire",
        "the switch did not put the call on the earpiece. {context}"
    );
    assert_eq!(
        value("audio-speaker"),
        "update:3,acquire",
        "the switch did not put the call back on the loudspeaker. {context}"
    );
    assert_eq!(
        value("audio-refused"),
        "update:1,release,unregister",
        "a set the policy refused was not given back. {context}"
    );
    assert_eq!(
        value("speaker-refused"),
        "false",
        "the switch is still there to use with nothing held. {context}"
    );
    assert_eq!(
        value("audio-over"),
        "",
        "a set already given back was given back again. {context}"
    );
    assert_eq!(
        value("audio-talking-over"),
        "false",
        "a call that is over still carries sound for the policy. {context}"
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
