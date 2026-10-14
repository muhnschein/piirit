// Viewfinder math, shared by camera page and scanner. Draws nothing.
// Tested alone: tests/qml_viewfinder_math.rs.
//
// Layout from jolla-camera, zoom from RAWfish (BSD). Nothing copied: plugins
// not importable by Harbour.
//
// Enums as values (QCamera's own): library can't import QtMultimedia, test
// stubs can't declare them.
.pragma library

/// `Camera.Position`: BackFace 1, FrontFace 2.
var backFace = 1
var frontFace = 2

/// `Camera.FlashMode`: QCameraExposure's flags.
var flashAuto = 1
var flashOff = 2
var flashOn = 4
var flashTorch = 32

/// `CameraImageProcessing.WhiteBalanceMode`.
var whiteBalanceAuto = 0
var whiteBalanceSunlight = 2
var whiteBalanceCloudy = 3
var whiteBalanceTungsten = 5
var whiteBalanceFluorescent = 6

/// `Camera.LockStatus`: Unlocked 0, Searching 1, Locked 2.
var lockLocked = 2

/// Silica `Orientation` bit flags, All = 15.
var portrait = 1
var landscape = 2
var portraitInverted = 4
var landscapeInverted = 8
var allOrientations = 15

/// White balance modes offered, platform order.
var whiteBalanceModes = [whiteBalanceAuto, whiteBalanceSunlight, whiteBalanceCloudy,
                         whiteBalanceFluorescent, whiteBalanceTungsten]

/// Self-timer delays, seconds.
var timerDelays = [0, 3, 10, 15]

/// Exposure compensation, EV, half-stop steps.
var exposureSteps = [-2, -1.5, -1, -0.5, 0, 0.5, 1, 1.5, 2]

/// Scanner zoom steps: 2x and 4x reach far codes (few pixels per module).
var zoomPresets = [1, 2, 4]

/// Device ids on one side, platform order.
///
/// `cameras`: `QtMultimedia.availableCameras`. No position = back.
function camerasFacing(cameras, position) {
    var ids = []
    if (!cameras) {
        return ids
    }
    for (var i = 0; i < cameras.length; i++) {
        var camera = cameras[i]
        if (!camera || !camera.deviceId) {
            continue
        }
        var facing = camera.position === frontFace ? frontFace : backFace
        if (facing === position) {
            ids.push("" + camera.deviceId)
        }
    }
    return ids
}

/// Switch-button target: from back, first front camera; from front, last back
/// camera used.
/// Empty if none: hides button.
function otherSide(cameras, currentPosition, lastBackId) {
    if (currentPosition === frontFace) {
        var backs = camerasFacing(cameras, backFace)
        if (backs.indexOf(lastBackId) >= 0) {
            return lastBackId
        }
        return backs.length > 0 ? backs[0] : ""
    }
    var fronts = camerasFacing(cameras, frontFace)
    return fronts.length > 0 ? fronts[0] : ""
}

/// Back-camera ring labels ("1.0", "0.5", "2.0") or list index.
///
/// `labels` in same order as `camerasFacing`.
function lensLabel(labels, index) {
    var label = labels && index < labels.length ? labels[index] : undefined
    if (label === undefined || label === null || ("" + label).length === 0) {
        return "" + (index + 1)
    }
    return "" + label
}

/// Silica page turn, degrees clockwise: 0/90/180/270 by orientation.
///
/// Viewfinder turned back by same: camera frame fixed to phone.
function pageTurn(orientation) {
    switch (orientation) {
    case landscape: return 90
    case portraitInverted: return 180
    case landscapeInverted: return 270
    default: return 0
    }
}

/// Pinch zoom: scale change times whole range. Clamped to [1, maximum].
function pinchZoom(current, maximum, scale, previousScale) {
    var top = Math.max(1, maximum || 1)
    var previous = Math.abs(previousScale) > 0 ? Math.abs(previousScale) : 1
    var next = current + (top - 1) * (scale / previous - 1)
    return clampZoom(next, top)
}

/// Zoom clamped to camera range.
function clampZoom(value, maximum) {
    var top = Math.max(1, maximum || 1)
    if (!isFinite(value)) {
        return 1
    }
    return Math.max(1, Math.min(top, value))
}

/// Next reachable preset above `current`, wrapping to 1.
function nextZoomPreset(current, maximum) {
    var top = Math.max(1, maximum || 1)
    for (var i = 0; i < zoomPresets.length; i++) {
        var preset = zoomPresets[i]
        if (preset > current + 0.05 && preset <= top + 0.05) {
            return Math.min(preset, top)
        }
    }
    return 1
}

/// Zoom text: at most one decimal, with "×" as lens labels.
function zoomText(zoom) {
    var rounded = Math.round(zoom * 10) / 10
    return (rounded % 1 === 0 ? rounded.toFixed(0) : rounded.toFixed(1)) + "×"
}

/// Indicator dot: 0 at no zoom, 1 at max. Halfway if camera can't zoom.
function zoomFraction(zoom, maximum) {
    if (!(maximum > 1)) {
        return 0.5
    }
    return Math.max(0, Math.min(1, (zoom - 1) / (maximum - 1)))
}

/// Flash modes: photo auto/off/on; video off/torch; none on front.
function flashModes(video, position) {
    if (position === frontFace) {
        return []
    }
    return video ? [flashOff, flashTorch] : [flashAuto, flashOff, flashOn]
}

/// Flash to run: `wanted` if offered, else off (video torch: no photo flash).
function effectiveFlash(wanted, video, position) {
    var modes = flashModes(video, position)
    if (modes.indexOf(wanted) >= 0) {
        return wanted
    }
    return flashOff
}

/// The platform's icon for a flash mode.
function flashIcon(mode) {
    switch (mode) {
    case flashAuto: return "image://theme/icon-camera-flash-automatic"
    // Torch uses flash's "on" icon: platform has none, its camera does same.
    case flashOn:
    case flashTorch: return "image://theme/icon-camera-flash-on"
    default: return "image://theme/icon-camera-flash-off"
    }
}

/// The platform's icon for a self-timer delay.
function timerIcon(seconds) {
    return seconds > 0 ? "image://theme/icon-camera-timer-" + seconds + "s"
                       : "image://theme/icon-camera-timer"
}

/// The platform's icon for a white balance mode.
function whiteBalanceIcon(mode) {
    switch (mode) {
    case whiteBalanceSunlight: return "image://theme/icon-camera-wb-sunny"
    case whiteBalanceCloudy: return "image://theme/icon-camera-wb-cloudy"
    case whiteBalanceFluorescent: return "image://theme/icon-camera-wb-fluorecent"
    case whiteBalanceTungsten: return "image://theme/icon-camera-wb-tungsten"
    default: return "image://theme/icon-camera-wb-automatic"
    }
}

/// The platform's icon for the viewfinder grid, on or off.
function gridIcon(on) {
    return on ? "image://theme/icon-camera-grid-thirds" : "image://theme/icon-camera-grid-none"
}

/// Exposure step as slider shows: signed, unitless.
function exposureText(value) {
    if (value === 0) {
        return "0"
    }
    return (value > 0 ? "+" : "−") + Math.abs(value)
}

/// Step nearest a slider fraction, 0 at top (brightest).
function exposureAt(fraction) {
    var last = exposureSteps.length - 1
    var index = Math.round(Math.max(0, Math.min(1, fraction)) * last)
    return exposureSteps[last - index]
}

/// Where a step sits on the slider's travel, 0 at the top.
function exposureFraction(value) {
    var index = exposureSteps.indexOf(value)
    if (index < 0) {
        index = exposureSteps.indexOf(0)
    }
    var last = exposureSteps.length - 1
    return (last - index) / last
}

/// Portrait height/width: long over short side, 4:3 until camera reports.
function frameRatio(width, height) {
    if (!(width > 0) || !(height > 0)) {
        return 4 / 3
    }
    return Math.max(width, height) / Math.min(width, height)
}

/// Self-timer seconds left, rounded up: 3, 2, 1, never 0.
function countdownText(remaining) {
    return "" + Math.max(1, Math.ceil(remaining))
}

/// Tap as focus point: fraction of frame. `mapped`: VideoOutput's tap mapping.
/// Tap outside picture (bar beside fitted frame): null, no focus.
function focusPoint(mapped, x, y, width, height) {
    var point = mapped
    if (!point || !isFinite(point.x) || !isFinite(point.y)) {
        if (!(width > 0) || !(height > 0)) {
            return null
        }
        point = { x: x / width, y: y / height }
    }
    if (point.x < 0 || point.x > 1 || point.y < 0 || point.y > 1) {
        return null
    }
    return { x: point.x, y: point.y }
}
