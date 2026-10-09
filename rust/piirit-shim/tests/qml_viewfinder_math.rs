//! The arithmetic both viewfinders share (`qml/js/Viewfinder.js`): which
//! camera the switch goes to, how far a pinch zooms, which flash modes a
//! camera offers, the scanner's zoom steps, the exposure slider's travel,
//! and where a tap lands in the camera's frame.
//!
//! Run in a real QML engine, because that is where the library runs: a
//! JavaScript port of it here would test the port.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use qmetaobject::*;
use serde_json::Value;

mod common;

/// Every case as `label: result`, worked out in one go and handed back as
/// JSON so that each can be asserted on its own.
fn probe(library: &str) -> String {
    format!(
        r#"
    import QtQuick 2.0
    import "{library}" as V
    Item {{
        function run() {{
            var cameras = [
                {{ deviceId: "back-0", position: 1 }},
                {{ deviceId: "front-0", position: 2 }},
                {{ deviceId: "back-1", position: 1 }},
                {{ deviceId: "mystery", position: 0 }},
                null,
                {{ position: 1 }}
            ]
            var out = {{}}
            out.backs = V.camerasFacing(cameras, V.backFace)
            out.fronts = V.camerasFacing(cameras, V.frontFace)
            out.none = V.camerasFacing(undefined, V.backFace)
            out.toFront = V.otherSide(cameras, V.backFace, "back-1")
            out.backToLast = V.otherSide(cameras, V.frontFace, "back-1")
            out.backToFirst = V.otherSide(cameras, V.frontFace, "gone")
            out.noFront = V.otherSide([{{ deviceId: "b", position: 1 }}], V.backFace, "")
            out.pinchOut = V.pinchZoom(1, 4, 2, 1)
            out.pinchStep = V.pinchZoom(2, 4, 1.1, 1.0)
            out.pinchIn = V.pinchZoom(2, 4, 0.5, 1)
            out.pinchPast = V.pinchZoom(3.9, 4, 3, 1)
            out.pinchFlat = V.pinchZoom(1, 1, 2, 1)
            out.pinchZeroPrevious = V.pinchZoom(1, 4, 1, 0)
            out.clampHigh = V.clampZoom(9, 4)
            out.clampLow = V.clampZoom(0.2, 4)
            out.clampNaN = V.clampZoom(NaN, 4)
            out.presets = [V.nextZoomPreset(1, 8), V.nextZoomPreset(2, 8),
                           V.nextZoomPreset(4, 8), V.nextZoomPreset(1.5, 8)]
            out.presetsCapped = [V.nextZoomPreset(1, 3), V.nextZoomPreset(2, 3),
                                 V.nextZoomPreset(3, 3)]
            out.presetsFlat = V.nextZoomPreset(1, 1)
            out.zoomTexts = [V.zoomText(1), V.zoomText(2.25), V.zoomText(3.999)]
            out.fractions = [V.zoomFraction(1, 4), V.zoomFraction(4, 4),
                             V.zoomFraction(2.5, 4), V.zoomFraction(2, 1)]
            out.stillFlash = V.flashModes(false, V.backFace)
            out.videoFlash = V.flashModes(true, V.backFace)
            out.frontFlash = V.flashModes(false, V.frontFace)
            out.effective = [V.effectiveFlash(V.flashOn, false, V.backFace),
                             V.effectiveFlash(V.flashTorch, false, V.backFace),
                             V.effectiveFlash(V.flashAuto, true, V.backFace),
                             V.effectiveFlash(V.flashOn, false, V.frontFace)]
            out.flashIcons = [V.flashIcon(V.flashAuto), V.flashIcon(V.flashOff),
                              V.flashIcon(V.flashOn), V.flashIcon(V.flashTorch)]
            out.timerIcons = [V.timerIcon(0), V.timerIcon(3), V.timerIcon(15)]
            out.wbIcons = V.whiteBalanceModes.map(V.whiteBalanceIcon)
            out.gridIcons = [V.gridIcon(false), V.gridIcon(true)]
            out.exposureTexts = [V.exposureText(0), V.exposureText(1.5), V.exposureText(-2)]
            out.exposureAt = [V.exposureAt(0), V.exposureAt(0.5), V.exposureAt(1),
                              V.exposureAt(-3), V.exposureAt(7), V.exposureAt(0.24)]
            out.exposureFraction = [V.exposureFraction(2), V.exposureFraction(0),
                                    V.exposureFraction(-2), V.exposureFraction(0.3)]
            out.countdown = [V.countdownText(3), V.countdownText(2.01), V.countdownText(0.2),
                             V.countdownText(0)]
            out.ratios = [V.frameRatio(1280, 960), V.frameRatio(960, 1280),
                          V.frameRatio(1920, 1080), V.frameRatio(0, 0)]
            out.focusMapped = V.focusPoint({{ x: 0.25, y: 0.75 }}, 10, 10, 100, 100)
            out.focusOutside = V.focusPoint({{ x: -0.1, y: 0.5 }}, 10, 10, 100, 100)
            out.focusRaw = V.focusPoint(null, 50, 25, 100, 100)
            out.focusNoSize = V.focusPoint(null, 50, 25, 0, 0)
            return JSON.stringify(out)
        }}
    }}
"#
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_viewfinder_arithmetic_agrees_with_the_platform_camera() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    let library = format!(
        "file://{}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/js/Viewfinder.js")
            .display()
    );

    let mut engine = QmlEngine::new();
    engine.load_data(QByteArray::from(probe(&library).as_str()));
    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut answer = String::new();
    let answer_ptr: *mut String = std::ptr::addr_of_mut!(answer);
    single_shot(Duration::from_millis(100), move || unsafe {
        let result = (*engine_ptr).invoke_method("run".into(), &[]);
        *answer_ptr = QString::from_qvariant(result)
            .map(|value| value.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });
    engine.exec();

    let out: Value = serde_json::from_str(&answer)
        .unwrap_or_else(|err| panic!("the library did not run ({err}): {answer:?}"));
    let at = |key: &str| out[key].clone();
    let number = |key: &str| out[key].as_f64().unwrap_or(f64::NAN);
    let close = |left: f64, right: f64| (left - right).abs() < 1e-9;

    assert_eq!(
        at("backs"),
        serde_json::json!(["back-0", "back-1", "mystery"]),
        "the back cameras are not the ones on the back, in the platform's order, \
         with a camera of no side taken as a back one"
    );
    assert_eq!(at("fronts"), serde_json::json!(["front-0"]));
    assert_eq!(at("none"), serde_json::json!([]), "no list is no cameras");
    assert_eq!(
        at("toFront"),
        "front-0",
        "the switch on the back does not go to the front"
    );
    assert_eq!(
        at("backToLast"),
        "back-1",
        "from the front the switch does not go back to the back camera last used"
    );
    assert_eq!(
        at("backToFirst"),
        "back-0",
        "a last back camera the phone no longer has is not replaced by its first"
    );
    assert_eq!(
        at("noFront"),
        "",
        "a phone with one side offers a switch to nothing"
    );

    // The platform's pinch: the change in scale times the whole range.
    assert!(
        close(number("pinchOut"), 4.0),
        "a doubling pinch on 1-4x: {}",
        at("pinchOut")
    );
    assert!(
        close(number("pinchStep"), 2.3),
        "a 10% spread at 2x: {}",
        at("pinchStep")
    );
    assert!(
        close(number("pinchIn"), 1.0),
        "a halving pinch at 2x: {}",
        at("pinchIn")
    );
    assert!(
        close(number("pinchPast"), 4.0),
        "a pinch past the maximum: {}",
        at("pinchPast")
    );
    assert!(
        close(number("pinchFlat"), 1.0),
        "a camera that cannot zoom zoomed: {}",
        at("pinchFlat")
    );
    assert!(
        close(number("pinchZeroPrevious"), 1.0),
        "a pinch with no previous scale is not read as none: {}",
        at("pinchZeroPrevious")
    );
    assert!(close(number("clampHigh"), 4.0));
    assert!(close(number("clampLow"), 1.0));
    assert!(
        close(number("clampNaN"), 1.0),
        "a zoom that is not a number is not one"
    );

    assert_eq!(
        at("presets"),
        serde_json::json!([2, 4, 1, 2]),
        "the scanner's steps are not one, two, four and back"
    );
    assert_eq!(
        at("presetsCapped"),
        serde_json::json!([2, 1, 1]),
        "a step past the camera's maximum was offered"
    );
    assert_eq!(at("presetsFlat"), 1);
    assert_eq!(
        at("zoomTexts"),
        serde_json::json!(["1\u{d7}", "2.3\u{d7}", "4\u{d7}"]),
        "the zoom is not written with one decimal and the platform's x"
    );
    assert_eq!(
        at("fractions"),
        serde_json::json!([0, 1, 0.5, 0.5]),
        "the indicator's dot is not where the zoom is"
    );

    assert_eq!(
        at("stillFlash"),
        serde_json::json!([1, 2, 4]),
        "a picture does not offer auto, off and on"
    );
    assert_eq!(
        at("videoFlash"),
        serde_json::json!([2, 32]),
        "a video does not offer off and the torch"
    );
    assert_eq!(
        at("frontFlash"),
        serde_json::json!([]),
        "the front camera offers a flash"
    );
    assert_eq!(
        at("effective"),
        serde_json::json!([4, 2, 2, 2]),
        "a flash mode not on offer was used rather than off"
    );
    assert_eq!(
        at("flashIcons"),
        serde_json::json!([
            "image://theme/icon-camera-flash-automatic",
            "image://theme/icon-camera-flash-off",
            "image://theme/icon-camera-flash-on",
            "image://theme/icon-camera-flash-on"
        ])
    );
    assert_eq!(
        at("timerIcons"),
        serde_json::json!([
            "image://theme/icon-camera-timer",
            "image://theme/icon-camera-timer-3s",
            "image://theme/icon-camera-timer-15s"
        ])
    );
    assert_eq!(
        at("wbIcons"),
        serde_json::json!([
            "image://theme/icon-camera-wb-automatic",
            "image://theme/icon-camera-wb-sunny",
            "image://theme/icon-camera-wb-cloudy",
            "image://theme/icon-camera-wb-fluorecent",
            "image://theme/icon-camera-wb-tungsten"
        ]),
        "the white balance icons are not the platform's (its \"fluorecent\" included)"
    );
    assert_eq!(
        at("gridIcons"),
        serde_json::json!([
            "image://theme/icon-camera-grid-none",
            "image://theme/icon-camera-grid-thirds"
        ])
    );

    assert_eq!(
        at("exposureTexts"),
        serde_json::json!(["0", "+1.5", "\u{2212}2"])
    );
    assert_eq!(
        at("exposureAt"),
        serde_json::json!([2, 0, -2, 2, -2, 1]),
        "the slider's top is not the brightest, or a drag past its ends is not held to them"
    );
    assert_eq!(
        at("exposureFraction"),
        serde_json::json!([0, 0.5, 1, 0.5]),
        "a value the slider cannot show does not sit at none"
    );
    assert_eq!(
        at("countdown"),
        serde_json::json!(["3", "3", "1", "1"]),
        "the self-timer does not count whole seconds up, down to one"
    );
    let ratios = out["ratios"].as_array().cloned().unwrap_or_default();
    let ratio = |index: usize| {
        ratios
            .get(index)
            .and_then(Value::as_f64)
            .unwrap_or(f64::NAN)
    };
    assert!(
        close(ratio(0), 4.0 / 3.0) && close(ratio(1), 4.0 / 3.0),
        "the frame is not drawn upright whichever way the source reports: {ratios:?}"
    );
    assert!(close(ratio(2), 16.0 / 9.0), "{ratios:?}");
    assert!(
        close(ratio(3), 4.0 / 3.0),
        "no source yet is not four by three: {ratios:?}"
    );

    assert_eq!(at("focusMapped"), serde_json::json!({"x": 0.25, "y": 0.75}));
    assert_eq!(
        at("focusOutside"),
        Value::Null,
        "a tap outside the picture asked for focus"
    );
    assert_eq!(at("focusRaw"), serde_json::json!({"x": 0.5, "y": 0.25}));
    assert_eq!(at("focusNoSize"), Value::Null);
}
