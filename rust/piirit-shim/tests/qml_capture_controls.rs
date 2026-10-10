//! The camera page's controls, the platform camera's set: the back
//! cameras' rings and the switch to the front, pinch zoom held inside what
//! the camera can do, the flash offered per mode and side, the settings
//! panel's white balance, grid and self-timer, the exposure slider, and a
//! tap's focus ring -- and while a video records, all of it out of the
//! way but the shutter. The rings wear the platform's lens labels, and
//! the page turns with the phone into the platform camera's landscape
//! layout, with the picture turned back.
//!
//! Driven through the `QtMultimedia` stubs: a phone with two cameras on the
//! back and one on the front unless a step says otherwise, which zooms
//! four times. What the page asked of the camera is read back off it.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use qmetaobject::*;
use serde_json::{json, Value};

mod common;

/// Silica's `pageStack`, counted rather than performed: the page pops
/// itself once it has a capture to report.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
    pops: qt_property!(i32),
    pop: qt_method!(fn(&mut self)),
}

impl StackProbe {
    fn pop(&mut self) {
        self.pops += 1;
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import QtMultimedia 5.6
    import Sailfish.Silica 1.0
    import 'file://__COMPONENTS__'
    Item {
        Loader { id: loader }
        property var defaultCameras
        Component.onCompleted: defaultCameras = QtMultimedia.availableCameras
        function load(url) {
            loader.setSource('', {})
            loader.setSource(url, {})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
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
        function item(name) {
            var hit = findIn(loader.item, name)
            if (!hit) { throw new Error('missing: ' + name) }
            return hit
        }
        function camera() { return item('camera') }
        // A tap on a MouseArea, or on an IconButton.
        function click(name) { item(name).clicked(null) }
        function tap(name) { item(name).clicked() }
        function rounded(p) { return [Math.round(p.x * 1000) / 1000, Math.round(p.y * 1000) / 1000] }

        function run(url) {
            var out = {}
            function step(label, work) {
                try { out[label] = work() } catch (err) { out[label] = 'threw: ' + err }
            }
            if (load(url) !== 'ok') { return JSON.stringify({ load: 'failed' }) }
            var page = loader.item
            var cam = camera()

            // What a phone with two back cameras and a front one offers.
            step('start', function() {
                return { device: cam.deviceId, lenses: item('lensToggle').visible,
                         flip: item('flipButton').visible, flash: cam.flash.mode }
            })
            // The rings wear the platform's label for each lens, and
            // their place where the phone gives none.
            step('labels', function() {
                var plain = [item('lensLabel0').text, item('lensLabel1').text]
                Settings.backLensLabelsConfig.value = ['1.0', '0.5']
                var labelled = [item('lensLabel0').text, item('lensLabel1').text]
                Settings.backLensLabelsConfig.value = []
                return { plain: plain, labelled: labelled }
            })
            // The viewfinder as it lies on the frame: how far it is turned,
            // its own size, and where its own top-left corner landed.
            function lie() {
                var v = item('viewfinder')
                var corner = v.mapToItem(item('frame'), 0, 0)
                return [v.rotation, v.width, v.height,
                        Math.round(corner.x), Math.round(corner.y)]
            }
            // Turned on its side: the picture turned back, the frame the
            // full height at the left, the controls in a column down the
            // right -- modes at the top, switch at the foot, rings beside
            // the shutter -- and the exposure slider at the picture's edge.
            step('landscape', function() {
                var frame = item('frame')
                var controls = item('controls')
                var shutter = item('shutter')
                var lenses = item('lensToggle')
                var slider = item('exposureSlider')
                page.width = 960
                page.height = 540
                page.orientation = 2
                var answer = {
                    allowed: page.allowedOrientations,
                    turn: lie(),
                    frame: [frame.x, frame.y, frame.width, frame.height],
                    controls: [controls.x, controls.y, controls.width, controls.height],
                    shutter: [shutter.x + shutter.width / 2, shutter.y + shutter.height / 2],
                    modesAbove: item('modeColumn').y + item('modeColumn').height <= shutter.y,
                    flipBelow: item('flipButton').y >= shutter.y + shutter.height,
                    lensesBeside: lenses.vertical && lenses.x + lenses.width <= shutter.x
                                  && lenses.y < shutter.y + shutter.height
                                  && lenses.y + lenses.height > shutter.y,
                    slider: slider.x + slider.width === frame.x + frame.width
                }
                // A tap near the frame's top left is near the picture's
                // top right, as the lens has it.
                page.focusAt(180, 135)
                answer.tap = rounded(camera().focus.customFocusPoint)
                answer.ring = [item('reticle').at.x, item('reticle').at.y]
                page.orientation = 8
                answer.inverted = lie()
                page.orientation = 1
                page.width = 540
                page.height = 960
                answer.upright = [lie(), frame.width, frame.height,
                                  controls.x, controls.width, lenses.vertical]
                return answer
            })
            // The frame: the full width, four by three, at the top of a
            // phone too short to push it down -- and below the settings
            // row on a tall one.
            step('frame', function() {
                var frame = item('frame')
                var low = [frame.width, frame.height, frame.y]
                page.height = 1200
                var tall = frame.y
                page.height = 960
                return { short: low, tall: tall }
            })

            step('zoom', function() {
                page.zoomTo(10)
                var high = cam.digitalZoom
                var shown = item('zoomIndicator').shown
                page.zoomTo(0.3)
                return [high, cam.digitalZoom, shown]
            })
            // Another lens starts over at no zoom.
            step('lens', function() {
                page.zoomTo(3)
                click('lens1')
                return [cam.deviceId, cam.digitalZoom]
            })
            // The front has no flash and one lens; back again is the lens
            // last used.
            step('front', function() {
                tap('flipButton')
                return { device: cam.deviceId, lenses: item('lensToggle').visible,
                         flash: cam.flash.mode, flashColumn: item('flashColumn').visible,
                         flashShown: item('flashIndicator').visible }
            })
            step('back', function() {
                tap('flipButton')
                return [cam.deviceId, item('lensToggle').visible, cam.flash.mode]
            })

            // The flash kept for a picture, the torch for a video only.
            step('flash', function() {
                var seen = []
                click('flashColumnOption2')
                seen.push(cam.flash.mode)
                tap('modeOption1')
                seen.push(cam.flash.mode)
                click('flashColumnOption1')
                seen.push(cam.flash.mode)
                tap('modeOption0')
                seen.push(cam.flash.mode)
                return seen
            })
            step('whiteBalance', function() {
                click('whiteBalanceColumnOption1')
                return [cam.imageProcessing.whiteBalanceMode, item('settingsHeader').text]
            })
            step('grid', function() {
                var before = item('grid').visible
                click('gridColumnOption1')
                var on = item('grid').visible
                click('gridColumnOption0')
                return [before, on, item('grid').visible]
            })
            step('exposure', function() {
                item('exposureSlider').setFraction(0)
                var top = cam.exposure.exposureCompensation
                item('exposureSlider').setFraction(0.75)
                return [top, cam.exposure.exposureCompensation, item('exposureValue').text]
            })

            // A tap: a ring under the finger, plain autofocus on the point
            // in the camera's frame, and the ring lit once it has locked.
            step('focus', function() {
                var reticle = item('reticle')
                var viewfinder = item('viewfinder')
                var before = reticle.visible
                page.focusAt(135, 360)
                var unlit = reticle.locked
                cam.lockStatus = 2
                return { before: before, shown: reticle.visible, locked: reticle.locked,
                         unlit: unlit,
                         centre: [reticle.x + reticle.width / 2, reticle.y + reticle.height / 2],
                         mode: cam.focus.focusMode, pointMode: cam.focus.focusPointMode,
                         point: rounded(cam.focus.customFocusPoint),
                         searches: cam.searches,
                         size: [viewfinder.width, viewfinder.height] }
            })
            step('panel', function() {
                var panel = item('settingsPanel')
                click('settingsRow')
                var opened = panel.open
                click('settingsDismiss')
                return [opened, panel.open]
            })

            // Recording: only the shutter and the zoom are left.
            step('recording', function() {
                tap('modeOption1')
                tap('shutter')
                return { recording: page.recording,
                         held: page.allowedOrientations,
                         panel: item('settingsPanel').visible,
                         exposure: item('exposureSlider').visible,
                         lenses: item('lensToggle').visible,
                         flip: item('flipButton').enabled,
                         modes: item('modeColumn').enabled }
            })

            // A phone with one camera: nothing to switch to.
            step('single', function() {
                QtMultimedia.availableCameras = [{ deviceId: 'back-0', position: 1 }]
                load(url)
                var answer = [item('flipButton').visible, item('lensToggle').visible]
                QtMultimedia.availableCameras = defaultCameras
                return answer
            })
            return JSON.stringify(out)
        }

        // The self-timer: set to three seconds, the shutter waits and
        // counts; a second tap calls it off; a third starts it again.
        function arm(url) {
            load(url)
            click('timerColumnOption1')
            tap('shutter')
            var asked = camera().imageCapture.requested
            var counting = item('countdown').visible
            tap('shutter')
            var cancelled = !item('countdown').visible
            tap('shutter')
            return JSON.stringify({ asked: asked, counting: counting, cancelled: cancelled,
                                    shown: item('countdown').text,
                                    face: page_timer_face() })
        }
        function page_timer_face() { return loader.item.counting }
        function fired() {
            var answer = JSON.stringify({ asked: camera().imageCapture.requested,
                                          counting: item('countdown').visible })
            click('timerColumnOption0')
            return answer
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_camera_page_has_the_platform_cameras_controls() {
    let temp = std::env::temp_dir().join(format!("piirit-capture-controls-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp dir");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("XDG_CACHE_HOME", &temp);
    }

    piirit_shim::register_qml_types();

    let stack_box = QObjectBox::new(StackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    let probe = PROBE_QML.replace("__COMPONENTS__", &components.display().to_string());
    engine.load_data(QByteArray::from(probe.as_str()));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut answers: Vec<String> = Vec::new();
    let answers_ptr: *mut Vec<String> = std::ptr::addr_of_mut!(answers);
    let url = common::page_url("CapturePage.qml");

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

    let first = url.clone();
    single_shot(Duration::from_millis(500), move || unsafe {
        (*answers_ptr).push(call!("run", QString::from(first.as_str())));
        (*answers_ptr).push(call!("arm", QString::from(first.as_str())));
    });
    single_shot(Duration::from_millis(4500), move || unsafe {
        (*answers_ptr).push(call!("fired"));
        (*engine_ptr).quit();
    });
    engine.exec();

    let parse = |index: usize| -> Value {
        let text = answers.get(index).cloned().unwrap_or_default();
        serde_json::from_str(&text).unwrap_or_else(|err| panic!("answer {index} ({err}): {text:?}"))
    };
    let out = parse(0);
    let context = format!("answers: {out}");

    assert_eq!(
        out["start"],
        json!({"device": "back-0", "lenses": true, "flip": true, "flash": 1}),
        "a phone with two back cameras and a front one does not offer the rings and the \
         switch, or the flash does not start on automatic. {context}"
    );
    assert_eq!(
        out["labels"],
        json!({"plain": ["1", "2"], "labelled": ["1.0", "0.5"]}),
        "the rings do not wear the platform's lens labels, or are not numbered where \
         the phone gives none. {context}"
    );
    assert_eq!(
        out["landscape"],
        json!({"allowed": 15, "turn": [-90, 540, 720, 0, 540],
               "frame": [0, 0, 720, 540], "controls": [720, 0, 240, 540],
               "shutter": [120, 270], "modesAbove": true, "flipBelow": true,
               "lensesBeside": true, "slider": true,
               "tap": [0.75, 0.25], "ring": [180, 135],
               "inverted": [-270, 540, 720, 720, 0],
               "upright": [[0, 540, 720, 0, 0], 540, 720, 0, 540, false]}),
        "on its side the page does not turn, the picture is not turned back against it \
         at the shape the frame has, a tap does not land on its point in the picture, or \
         the frame and the controls are not laid out the platform camera's landscape \
         way. {context}"
    );
    assert_eq!(
        out["frame"],
        json!({"short": [540, 720, 0], "tall": 120}),
        "the frame is not the full width at four by three, at the top -- or, on a tall \
         phone, not pushed down below the settings row. {context}"
    );
    assert_eq!(
        out["zoom"],
        json!([4, 1, true]),
        "a zoom was asked for outside what the camera can do, or the zoom line did not \
         show. {context}"
    );
    assert_eq!(
        out["lens"],
        json!(["back-1", 1]),
        "the second ring did not change to the second back camera at no zoom. {context}"
    );
    assert_eq!(
        out["front"],
        json!({"device": "front-0", "lenses": false, "flash": 2,
               "flashColumn": false, "flashShown": false}),
        "the front camera was not switched to, or it still offers lenses or a flash. \
         {context}"
    );
    assert_eq!(
        out["back"],
        json!(["back-1", true, 1]),
        "back from the front is not the back camera last used, with its flash. {context}"
    );
    assert_eq!(
        out["flash"],
        json!([4, 2, 32, 4]),
        "the flash picked for a picture did not reach the camera, a video did not start \
         with it off and offer the torch, or the torch was carried into a picture. \
         {context}"
    );
    assert_eq!(
        out["whiteBalance"],
        json!([2, "Sunny"]),
        "the white balance picked did not reach the camera, or the header did not say \
         which it is. {context}"
    );
    assert_eq!(
        out["grid"],
        json!([false, true, false]),
        "the grid is not off until asked for, then on, then off again. {context}"
    );
    assert_eq!(
        out["exposure"],
        json!([2, -1, "\u{2212}1"]),
        "the exposure slider does not reach the camera, brighter at the top. {context}"
    );
    assert_eq!(
        out["focus"],
        json!({"before": false, "shown": true, "locked": true, "unlit": false,
               "centre": [135, 360], "mode": 8, "pointMode": 3,
               "point": [0.25, 0.5], "searches": 2, "size": [540, 720]}),
        "a tap did not focus on its point in plain autofocus with a ring under the finger \
         that lights once the lens locks. {context}"
    );
    assert_eq!(
        out["panel"],
        json!([true, false]),
        "the settings row does not pull the panel down, or a tap outside does not put it \
         away. {context}"
    );
    assert_eq!(
        out["recording"],
        json!({"recording": true, "held": 1, "panel": false, "exposure": false, "lenses": false,
               "flip": false, "modes": false}),
        "while a video records, more than the shutter and the zoom is left to tap, or \
         the page still turns with the phone. {context}"
    );
    assert_eq!(
        out["single"],
        json!([false, false]),
        "a phone with one camera offers a switch or rings. {context}"
    );

    let armed = parse(1);
    assert_eq!(
        armed,
        json!({"asked": "", "counting": true, "cancelled": true, "shown": "3", "face": true}),
        "with the self-timer at three seconds the shutter took the picture at once, did \
         not count, or could not be called off: {armed}"
    );
    let fired = parse(2);
    assert!(
        fired["asked"]
            .as_str()
            .is_some_and(|path| path.contains("/captures/photo-")),
        "the self-timer ran out and no picture was asked for: {fired}"
    );
    assert_eq!(
        fired["counting"],
        json!(false),
        "the count stayed up: {fired}"
    );
}
