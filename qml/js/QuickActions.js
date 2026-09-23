// What a quick action on the cover is, and what it is drawn as, for the
// three places that need it: the cover, which offers the actions; the
// window, which does them; and the settings page, which picks them.
//
// A library for the reason Relays.js is one: nothing here is translated,
// and the lists are shared rather than copied.
.pragma library

/// What an action can do, as Settings keeps it: open a chat, search the
/// chat list, show this profile's QR code, or read someone else's.
var kinds = ["chat", "search", "qr", "scan"]

/// The icons an action for a chat can wear, in the order the settings page
/// offers them. The first is the one a new action gets.
var chatIcons = ["heart", "star", "person", "group", "home", "bubble"]

/// The sizes scripts/render-cover-icons.sh draws every icon at: Silica's
/// small icon at each scale a phone runs at, 1.0 to 2.0.
var sizes = [32, 40, 48, 56, 64]

/// One side's action, read out of Settings: `side` is "left" or "right".
///
/// A kind this version does not know is no action. dconf answers
/// `undefined` for a key it has not read yet, which is none as well.
function read(settings, side) {
    var prefix = side === "right" ? "quickActionRight" : "quickActionLeft"
    var kind = "" + settings[prefix]
    return {
        kind: kinds.indexOf(kind) >= 0 ? kind : "",
        accountId: settings[prefix + "Account"] > 0 ? settings[prefix + "Account"] : 0,
        chatId: settings[prefix + "Chat"] > 0 ? settings[prefix + "Chat"] : 0,
        icon: chatIcons.indexOf("" + settings[prefix + "Icon"]) >= 0
              ? "" + settings[prefix + "Icon"] : chatIcons[0]
    }
}

/// The icon an action wears: the one picked for a chat, and otherwise the
/// kind's own, whose file is named after it.
function iconName(action) {
    return action.kind === "chat" ? action.icon : action.kind
}

/// Where an icon's picture is, relative to qml/: drawn at the size nearest
/// `size`, in white for a dark ambience and black for a light one.
///
/// The home screen draws a cover action's picture itself, as the file is,
/// so the file has to be the one that fits and shows.
function iconFile(name, size, onDark) {
    var best = sizes[0]
    for (var i = 1; i < sizes.length; i++) {
        if (Math.abs(sizes[i] - size) < Math.abs(best - size)) {
            best = sizes[i]
        }
    }
    return "art/cover/" + name + "-" + best + "-" + (onDark ? "white" : "black") + ".png"
}

/// Whether text in `colour` is light, which is what a dark ambience draws
/// in: the ambience's own primary colour says which kind it is.
function isLight(colour) {
    return 0.299 * colour.r + 0.587 * colour.g + 0.114 * colour.b > 0.5
}
