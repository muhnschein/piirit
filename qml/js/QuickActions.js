// What a quick action on the cover is, and what it is drawn as, for the
// three places that need it: the cover, which offers the actions; the
// window, which does them; and the settings page, which picks them.
//
// A library for the reason Relays.js is one: nothing here is translated,
// and the lists are shared rather than copied.
.pragma library

/// What an action can do, as Settings keeps it: open a chat, search the
/// chat list, show this profile's QR code, read someone else's, or open
/// the list of profiles.
var kinds = ["chat", "search", "qr", "scan", "profiles"]

/// The icons an action for a chat can wear, in the order the settings page
/// offers them. The first is the one a new action gets.
var chatIcons = ["heart", "star", "person", "group", "home", "bubble", "robot", "school",
                 "work", "office", "dog", "cat", "ball"]

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

/// How many actions the reader chose to have: 2, or 1 for anything else,
/// a key dconf has not read yet included.
function count(settings) {
    return settings.quickActionCount === 2 ? 2 : 1
}

/// The action the cover offers on `side`: what `read` says, except that
/// with one action chosen there is none on the right. What was set up
/// there stays in the settings, for when two are chosen again.
function shown(settings, side) {
    var action = read(settings, side)
    if (side === "right" && count(settings) < 2) {
        action.kind = ""
    }
    return action
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

/// The fade that makes room for the actions: whatever it is drawn over
/// runs out towards the bottom edge, from `fadeFrom` down to `fadeTo`,
/// eased rather than straight, as vuo's cover eases its texture away --
/// most of the way down the faces keep their strength, and they give up
/// the rest near the bottom. For a layer's ShaderEffect with those two
/// and `gridHeight`, the layer's height, in pixels: the cover's grid, and
/// the previews on the quick actions' page, which are drawn the same way
/// so they look the same.
///
/// highp for the ramp, which runs over a good part of the cover and would
/// band in steps at lowp. The colour is premultiplied, so the whole of it
/// goes with the fade. Pieced together line by line rather than written as
/// one string over several lines, which QML takes and a script need not.
var fadeShader =
    "varying highp vec2 qt_TexCoord0;\n" +
    "uniform sampler2D source;\n" +
    "uniform highp float fadeFrom;\n" +
    "uniform highp float fadeTo;\n" +
    "uniform highp float gridHeight;\n" +
    "uniform lowp float qt_Opacity;\n" +
    "void main() {\n" +
    "    highp float y = qt_TexCoord0.y * gridHeight;\n" +
    "    highp float sink = clamp((fadeTo - y) / max(1.0, fadeTo - fadeFrom), 0.0, 1.0);\n" +
    "    gl_FragColor = texture2D(source, qt_TexCoord0) * (sink * sink) * qt_Opacity;\n" +
    "}\n"
