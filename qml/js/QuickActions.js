// Quick action on cover. Used by cover (offers), window (does), settings
// (picks). Cover's call action drawn the same way, same files.
//
// Library: nothing translated, lists shared (as Relays.js).
.pragma library

/// Kinds Settings stores. Photo/video sent to chat picked after.
var kinds = ["chat", "search", "qr", "scan", "profiles",
             "photo", "video", "selfie", "selfie-video"]

/// Camera kinds. `mode`: 0 still, 1 video. `front`: front cam.
var captures = {
    "photo": { mode: 0, front: false },
    "video": { mode: 1, front: false },
    "selfie": { mode: 0, front: true },
    "selfie-video": { mode: 1, front: true }
}

/// Whether an action of `kind` starts with the camera.
function isCapture(kind) {
    return Object.keys(captures).indexOf(kind) >= 0
}

/// Chat icons, settings order. First = default.
var chatIcons = ["heart", "star", "person", "group", "home", "bubble", "robot", "school",
                 "work", "office", "dog", "cat", "ball"]

/// Call-time cover icons, not user-picked. `mic-off` while on, `mic` off.
var callIcons = ["mic", "mic-off"]

/// Sizes render-cover-icons.sh draws at: Silica small icon, scales 1.0-2.0.
var sizes = [32, 40, 48, 56, 64]

/// One side's action from Settings. `side`: left/right.
///
/// Unknown kind, or unread dconf `undefined`: no action.
function read(settings, side) {
    var prefix = side === "right" ? "quickActionRight" : "quickActionLeft"
    var kind = "" + settings[prefix]
    return {
        kind: kinds.indexOf(kind) >= 0 ? kind : "",
        accountId: settings[prefix + "Account"] > 0 ? settings[prefix + "Account"] : 0,
        chatId: settings[prefix + "Chat"] > 0 ? settings[prefix + "Chat"] : 0,
        icon: chatIcons.indexOf("" + settings[prefix + "Icon"]) >= 0
              ? "" + settings[prefix + "Icon"] : chatIcons[0],
        // Only `true` counts. Unread key = no.
        sendNow: settings[prefix + "SendNow"] === true
    }
}

/// Actions chosen: 2 or 1. Unread key = 1.
function count(settings) {
    return settings.quickActionCount === 2 ? 2 : 1
}

/// Action on `side`: `read`, but right blanked when one chosen.
/// Right setup kept for when two again.
function shown(settings, side) {
    var action = read(settings, side)
    if (side === "right" && count(settings) < 2) {
        action.kind = ""
    }
    return action
}

/// Icon name: chat's picked icon, else kind (file name).
function iconName(action) {
    return action.kind === "chat" ? action.icon : action.kind
}

/// Icon path rel. to qml/: nearest `size`, white if dark ambience else black.
///
/// Home screen draws file as-is, so size must match.
function iconFile(name, size, onDark) {
    var best = sizes[0]
    for (var i = 1; i < sizes.length; i++) {
        if (Math.abs(sizes[i] - size) < Math.abs(best - size)) {
            best = sizes[i]
        }
    }
    return "art/cover/" + name + "-" + best + "-" + (onDark ? "white" : "black") + ".png"
}

/// Whether `colour` is light. Dark ambience draws light text.
function isLight(colour) {
    return 0.299 * colour.r + 0.587 * colour.g + 0.114 * colour.b > 0.5
}

/// Fade under actions: `from` down to `to`, eased like vuo cover.
/// `at` in same units as `from`/`to`. Used by cover grid and quick-action
/// previews. Same curve as src/pictures.rs face fade.
///
/// Was ShaderEffect. See Avatar.qml: no shaders.
function fade(at, from, to) {
    if (!(to > from)) {
        return 1
    }
    var sink = Math.max(0, Math.min(1, (to - at) / (to - from)))
    return sink * sink
}
