// What the two viewfinders -- the camera page and the code scanner -- work
// out about the camera without drawing anything: which cameras the phone
// has and which comes next, how far a pinch zooms, which flash modes and
// self-timer delays are offered, and where a tap lands in the camera's
// own frame.
//
// A library rather than functions on each view, so the two agree and the
// arithmetic is tested on its own (tests/qml_viewfinder_math.rs).
//
// Layout and choices follow the platform's camera app (jolla-camera, BSD);
// the eased zoom steps follow RAWfish's ZoomController (BSD). Neither's
// code is copied: both lean on a camera plugin of their own that a
// Harbour app cannot import, so what is here is the part that QtMultimedia
// alone can do.
//
// QtMultimedia's enums are written as their values: a library cannot
// import the module that declares them, and the headless stubs the tests
// run on cannot declare a property of one. The values are QCamera's own.
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

/// Silica's `Orientation`: Portrait 1, Landscape 2, PortraitInverted 4,
/// LandscapeInverted 8, and All of them, 15.
var portrait = 1
var landscape = 2
var portraitInverted = 4
var landscapeInverted = 8
var allOrientations = 15

/// The white balance modes offered, in the platform camera's order.
var whiteBalanceModes = [whiteBalanceAuto, whiteBalanceSunlight, whiteBalanceCloudy,
                         whiteBalanceFluorescent, whiteBalanceTungsten]

/// The self-timer's delays in seconds, the platform camera's four.
var timerDelays = [0, 3, 10, 15]

/// Exposure compensation in EV, half a stop at a time either side of
/// none: the platform camera's range.
var exposureSteps = [-2, -1.5, -1, -0.5, 0, 0.5, 1, 1.5, 2]

/// The zoom presets the scanner's button steps through. A code across the
/// room is the case it is for: two and four times bring it to where a
/// decoder has a few pixels a module again.
var zoomPresets = [1, 2, 4]

/// The cameras on one side of the phone, in the order the platform lists
/// them, as their device ids.
///
/// `cameras` is `QtMultimedia.availableCameras`: a list of objects with a
/// `deviceId` and a `position`. A camera with no position is taken to be
/// on the back, where the platform's own app puts it.
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

/// The camera the switch button goes to: the first front camera from the
/// back, the last back camera used from the front. Empty when the phone
/// has no camera on the other side, which hides the button.
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

/// What the back cameras' rings say: the platform's label for each lens
/// ("1.0" for the main one, "0.5" or "0.6" for the wide one, "2.0" for a
/// telephoto) where the phone's adaptation gives one, and its place in
/// the list where it does not.
///
/// `labels` is that list, in the order the platform lists the back
/// cameras -- the order `camerasFacing` keeps.
function lensLabel(labels, index) {
    var label = labels && index < labels.length ? labels[index] : undefined
    if (label === undefined || label === null || ("" + label).length === 0) {
        return "" + (index + 1)
    }
    return "" + label
}

/// How far, in degrees clockwise, Silica turns a page held in
/// `orientation`: none upright, a quarter for landscape, half for upside
/// down, three quarters for the other landscape -- the turn the platform
/// camera reads off the page.
///
/// The viewfinder turns the picture back by as much: the camera's frame
/// is fixed to the phone, and a page that turns with the phone would take
/// the picture round with it. `VideoOutput.orientation` counts the other
/// way, anticlockwise, so the same number undoes the page's turn.
function pageTurn(orientation) {
    switch (orientation) {
    case landscape: return 90
    case portraitInverted: return 180
    case landscapeInverted: return 270
    default: return 0
    }
}

/// The zoom a pinch leads to: the platform camera's rule, which moves the
/// zoom by the pinch's change in scale times the whole range, so one
/// spread of the fingers covers it whatever the camera's maximum is.
/// Never below one, never past the maximum.
function pinchZoom(current, maximum, scale, previousScale) {
    var top = Math.max(1, maximum || 1)
    var previous = Math.abs(previousScale) > 0 ? Math.abs(previousScale) : 1
    var next = current + (top - 1) * (scale / previous - 1)
    return clampZoom(next, top)
}

/// A zoom kept inside what the camera can do.
function clampZoom(value, maximum) {
    var top = Math.max(1, maximum || 1)
    if (!isFinite(value)) {
        return 1
    }
    return Math.max(1, Math.min(top, value))
}

/// The preset after `current`, among those the camera can reach, back to
/// one after the last. A zoom between presets goes to the next one up.
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

/// How a zoom is written beside the indicator: one decimal, the "x" the
/// platform's lens labels wear.
function zoomText(zoom) {
    var rounded = Math.round(zoom * 10) / 10
    return (rounded % 1 === 0 ? rounded.toFixed(0) : rounded.toFixed(1)) + "×"
}

/// Where the dot sits on the indicator's line, from 0 at no zoom to 1 at
/// the camera's most. Halfway when the camera cannot zoom at all, as the
/// platform's indicator has it.
function zoomFraction(zoom, maximum) {
    if (!(maximum > 1)) {
        return 0.5
    }
    return Math.max(0, Math.min(1, (zoom - 1) / (maximum - 1)))
}

/// The flash modes offered: auto, off and on for a picture; off and the
/// torch for a video, since a flash fired once does nothing for one; and
/// none on the front, which has no flash to fire.
function flashModes(video, position) {
    if (position === frontFace) {
        return []
    }
    return video ? [flashOff, flashTorch] : [flashAuto, flashOff, flashOn]
}

/// The flash mode to run with: the one asked for when it is on offer here,
/// off otherwise -- the torch left on from a video is not a flash for a
/// picture.
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
    // The torch wears the flash's own "on": the platform has no icon of
    // its own for it, and its camera does the same.
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

/// An exposure step as the slider shows it: signed, no unit, as the
/// platform's slider writes it.
function exposureText(value) {
    if (value === 0) {
        return "0"
    }
    return (value > 0 ? "+" : "−") + Math.abs(value)
}

/// The step nearest a fraction of the slider's travel, 0 at the top.
/// The top is the brightest, as the platform's slider has it.
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

/// How tall the viewfinder is for its width in portrait: the source's
/// long side over its short one, or four by three -- the platform
/// camera's default -- while the camera has not said.
function frameRatio(width, height) {
    if (!(width > 0) || !(height > 0)) {
        return 4 / 3
    }
    return Math.max(width, height) / Math.min(width, height)
}

/// The seconds left of a self-timer, as the shutter shows them: whole
/// seconds, rounded up, so it reads 3, 2, 1 and never 0.
function countdownText(remaining) {
    return "" + Math.max(1, Math.ceil(remaining))
}

/// A tap on the viewfinder as the focus point the camera wants: a
/// fraction of the frame, inside it. `mapped` is what the VideoOutput made
/// of the tap, when it could say; a tap outside the picture -- in a bar
/// beside a fitted frame -- comes back null and asks for no focus.
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
