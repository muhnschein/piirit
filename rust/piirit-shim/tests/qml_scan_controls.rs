//! The scanner's camera controls: a pinch or the zoom button brings a
//! small code closer, never past what the camera can do; the torch lights
//! a code on paper; the rings and the switch change cameras, which starts
//! the zoom over and puts the torch out on the front; and a tap focuses on
//! its point, with the ring the camera page has.
//!
//! The decoding itself is `qml_scan.rs`'s. This is what is around it.

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
use serde_json::{json, Value};

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import QtMultimedia 5.6
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader; width: 540; height: 960 }
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
        function click(name) { item(name).clicked(null) }
        function tap(name) { item(name).clicked() }

        function run(url) {
            var out = {}
            function step(label, work) {
                try { out[label] = work() } catch (err) { out[label] = 'threw: ' + err }
            }
            if (load(url) !== 'ok') { return JSON.stringify({ load: 'failed' }) }
            var view = loader.item
            var cam = item('camera')
            view.active = true

            step('start', function() {
                return { zoom: cam.digitalZoom, label: item('zoomLabel').text,
                         torch: item('torchButton').visible, lenses: item('lensToggle').visible,
                         flip: item('flipButton').visible, flash: cam.flash.mode,
                         controls: item('cameraControls').visible }
            })
            // One, two, four and back, each shown on the button.
            step('steps', function() {
                var seen = []
                for (var i = 0; i < 4; i++) {
                    click('zoomButton')
                    seen.push([cam.digitalZoom, item('zoomLabel').text])
                }
                return seen
            })
            step('clamped', function() {
                view.zoomTo(12)
                var high = cam.digitalZoom
                var shown = item('zoomIndicator').shown
                view.zoomTo(-1)
                return [high, cam.digitalZoom, shown]
            })
            step('torch', function() {
                tap('torchButton')
                var on = cam.flash.mode
                return [on, item('torchButton').icon.source.toString()]
            })
            // The second back camera: no zoom; the torch stays lit.
            step('lens', function() {
                view.zoomTo(2)
                click('lens1')
                return [cam.deviceId, cam.digitalZoom, cam.flash.mode]
            })
            // The front: no torch, no rings; back is the lens last used,
            // still lit.
            step('front', function() {
                tap('flipButton')
                return { device: cam.deviceId, flash: cam.flash.mode,
                         torch: item('torchButton').visible,
                         lenses: item('lensToggle').visible }
            })
            step('back', function() {
                tap('flipButton')
                return [cam.deviceId, cam.flash.mode]
            })
            step('focus', function() {
                var reticle = item('reticle')
                view.tapAt(270, 240)
                var point = cam.focus.customFocusPoint
                return { shown: reticle.visible,
                         grabbed: reticle.parent === item('viewfinder'),
                         centre: [reticle.x + reticle.width / 2, reticle.y + reticle.height / 2],
                         point: [point.x, point.y], mode: cam.focus.focusMode }
            })
            // Typing the link puts the camera's controls away.
            step('typing', function() {
                tap('typeLinkButton')
                return item('cameraControls').visible
            })
            step('single', function() {
                QtMultimedia.availableCameras = [{ deviceId: 'back-0', position: 1 }]
                load(url)
                var answer = [item('flipButton').visible, item('lensToggle').visible,
                              item('torchButton').visible]
                QtMultimedia.availableCameras = defaultCameras
                return answer
            })
            return JSON.stringify(out)
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_scanner_zooms_lights_and_switches_cameras() {
    let temp = std::env::temp_dir().join(format!("piirit-scan-controls-{}", std::process::id()));
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
    let mut answer = String::new();
    let answer_ptr: *mut String = std::ptr::addr_of_mut!(answer);
    let url = common::component_url("ScanView.qml");
    single_shot(Duration::from_millis(500), move || unsafe {
        let result = (*engine_ptr)
            .invoke_method("run".into(), &[QVariant::from(QString::from(url.as_str()))]);
        *answer_ptr = QString::from_qvariant(result)
            .map(|value| value.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });
    engine.exec();

    let out: Value = serde_json::from_str(&answer)
        .unwrap_or_else(|err| panic!("the probe did not run ({err}): {answer:?}"));
    let context = format!("answers: {out}");

    assert_eq!(
        out["start"],
        json!({"zoom": 1, "label": "1\u{d7}", "torch": true, "lenses": true, "flip": true,
               "flash": 2, "controls": true}),
        "the scanner does not start unzoomed with the torch off and its controls up. \
         {context}"
    );
    assert_eq!(
        out["steps"],
        json!([
            [2, "2\u{d7}"],
            [4, "4\u{d7}"],
            [1, "1\u{d7}"],
            [2, "2\u{d7}"]
        ]),
        "the zoom button does not step one, two, four and back. {context}"
    );
    assert_eq!(
        out["clamped"],
        json!([4, 1, true]),
        "the scanner asked for a zoom the camera cannot do, or did not show it. {context}"
    );
    assert_eq!(
        out["torch"],
        json!([32, "image://theme/icon-camera-flash-on"]),
        "the torch button did not light the torch. {context}"
    );
    assert_eq!(
        out["lens"],
        json!(["back-1", 1, 32]),
        "the second ring did not change to the second back camera at no zoom, with the \
         torch kept. {context}"
    );
    assert_eq!(
        out["front"],
        json!({"device": "front-0", "flash": 2, "torch": false, "lenses": false}),
        "the front camera kept the torch, or offers it or the rings. {context}"
    );
    assert_eq!(
        out["back"],
        json!(["back-1", 32]),
        "back from the front is not the lens last used with its torch. {context}"
    );
    assert_eq!(
        out["focus"],
        json!({"shown": true, "grabbed": false, "centre": [270, 240], "point": [0.5, 0.25],
               "mode": 8}),
        "a tap did not focus on its point, in plain autofocus, under a ring -- or the \
         ring is drawn inside the viewfinder, where every grabbed frame carries it over \
         the code. {context}"
    );
    assert_eq!(
        out["typing"],
        json!(false),
        "the camera's controls stayed up over the link panel. {context}"
    );
    assert_eq!(
        out["single"],
        json!([false, false, true]),
        "a phone with one camera offers a switch or rings, or lost its torch. {context}"
    );
}
