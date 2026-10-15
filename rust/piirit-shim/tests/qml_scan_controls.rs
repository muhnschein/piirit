//! Scanner camera controls: pinch brings small code closer, never past camera's max;
//! torch lights code on paper; tap focuses on its point with camera page's ring. Only
//! torch and link button, one row: no lens rings, zoom steps or camera switch (camera
//! page's), default camera kept. On page turned with phone, picture is turned back.
//!
//! Decoding itself: `qml_scan.rs`. This is what is around it.

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
    import 'file://__COMPONENTS__'
    Item {
        Loader { id: loader; width: 540; height: 960 }
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
                return { zoom: cam.digitalZoom, device: cam.deviceId,
                         torch: item('torchButton').visible, flash: cam.flash.mode,
                         controls: item('cameraControls').visible }
            })
            // Torch and link button share the one row.
            step('row', function() {
                var torch = item('torchButton')
                var link = item('typeLinkButton')
                var t = torch.mapToItem(view, 0, torch.height / 2)
                var l = link.mapToItem(view, 0, link.height / 2)
                return { same: Math.round(t.y) === Math.round(l.y),
                         apart: t.x + torch.width < l.x,
                         centred: Math.round(l.x + link.width / 2) === view.width / 2 }
            })
            step('gone', function() {
                return ['lensToggle', 'zoomButton', 'flipButton'].map(function(name) {
                    return findIn(view, name) === null
                })
            })
            step('clamped', function() {
                view.zoomTo(12)
                var high = cam.digitalZoom
                var shown = item('zoomIndicator').shown
                view.zoomTo(-1)
                return [high, cam.digitalZoom, shown]
            })
            // On a page turned with the phone, the picture is turned back.
            step('turned', function() {
                function lie() {
                    var v = item('viewfinder')
                    return [v.rotation, v.width, v.height]
                }
                var seen = [lie()]
                loader.width = 960
                loader.height = 540
                view.pageOrientation = 2
                seen.push(lie())
                // A tap on the turned picture puts the ring under the
                // finger on the view.
                var v = item('viewfinder')
                var finger = v.mapToItem(view, 100, 200)
                view.tapAt(100, 200)
                var ring = item('reticle')
                seen.push([Math.round(finger.x), Math.round(finger.y),
                           ring.x + ring.width / 2, ring.y + ring.height / 2])
                view.pageOrientation = 8
                seen.push(lie())
                view.pageOrientation = 1
                loader.width = 540
                loader.height = 960
                seen.push(lie())
                return seen
            })
            step('torch', function() {
                tap('torchButton')
                var on = cam.flash.mode
                return [on, item('torchButton').icon.source.toString()]
            })
            step('off', function() {
                tap('torchButton')
                return [cam.flash.mode, item('torchButton').icon.source.toString()]
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
            // No link to type: torch alone.
            step('noLink', function() {
                view.typing = false
                view.offerLink = false
                return [item('cameraControls').visible, item('torchButton').visible,
                        item('typeLinkButton').visible]
            })
            return JSON.stringify(out)
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_scanner_zooms_and_lights_with_only_the_torch_and_link_button() {
    let temp = std::env::temp_dir().join(format!("piirit-scan-controls-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp dir");

    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("XDG_CACHE_HOME", &temp);
    }

    piirit_shim::register_qml_types();

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    let probe = PROBE_QML.replace("__COMPONENTS__", &components.display().to_string());
    engine.load_data(QByteArray::from(probe.as_str()));

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
        json!({"zoom": 1, "device": "back-0", "torch": true, "flash": 2, "controls": true}),
        "the scanner does not start on the default camera, unzoomed, with the torch off \
         and its controls up. {context}"
    );
    assert_eq!(
        out["row"],
        json!({"same": true, "apart": true, "centred": true}),
        "the torch and the link button are not one row, the button centred clear of the \
         torch. {context}"
    );
    assert_eq!(
        out["gone"],
        json!([true, true, true]),
        "the scanner still has lens rings, a zoom button or a camera switch. {context}"
    );
    assert_eq!(
        out["clamped"],
        json!([4, 1, true]),
        "the scanner asked for a zoom the camera cannot do, or did not show it. {context}"
    );
    assert_eq!(
        out["turned"],
        json!([
            [0, 540, 960],
            [-90, 540, 960],
            [200, 440, 200, 440],
            [-270, 540, 960],
            [0, 540, 960]
        ]),
        "the scanner's picture is not turned back on a page turned with the phone. \
         {context}"
    );
    assert_eq!(
        out["torch"],
        json!([32, "image://theme/icon-camera-flash-on"]),
        "the torch button did not light the torch. {context}"
    );
    assert_eq!(
        out["off"],
        json!([2, "image://theme/icon-camera-flash-off"]),
        "a second tap on the torch button did not put the torch out. {context}"
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
        out["noLink"],
        json!([true, true, false]),
        "with no link to type, the torch went too, or the link button stayed. {context}"
    );
}
