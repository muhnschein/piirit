// Emoji picture lookup, and emoji scan of messages.
//
// Qt 5.6 has no colour fonts: emoji with a shipped Twemoji picture draw as one,
// else text (components/EmojiGlyph.qml). List generated:
// scripts/fetch-emoji.py.
//
// Shared library: nothing translates; tables read once, not per row. Drawing
// uses only EmojiPictures.js, built ahead by the script: Qt 5.6 keeps no
// compiled scripts, and building tables from the full list (~3x size) cost the
// first chat opened each launch. Full list is EmojiPicker.js's alone.
//
// Tables:
// - `byKey`: picture per `key`.
// - `bmpStarts`: first char of each BMP emoji: scan stops only there and at
//   astral chars.
// - `textStyle`: text-default symbols (heart, (c), arrow), emoji only with
//   U+FE0F (as Unicode and other clients). BMP only: above it, data spells even
//   thumbs-up with the selector.
// - `mayHold`: text needs an emoji key char to be worth scanning.
.pragma library
.import "EmojiPictures.js" as Pictures

/// Emoji with variation selectors (U+FE0F) removed: same emoji comes with or
/// without.
function key(emoji) {
    return emoji.replace(/️/g, "")
}

/// Same, with skin tones removed too.
function untoned(emoji) {
    return key(emoji).replace(/\uD83C[\uDFFB-\uDFFF]/g, "")
}

/// Picture file name under qml/art/emoji/ (no extension), or "" if none. Skin
/// tones not shipped: toned emoji drawn untoned.
function pictureFor(emoji) {
    if (!emoji) {
        return ""
    }
    var known = Pictures.byKey
    var name = known[key(emoji)]
    if (name === undefined) {
        name = known[untoned(emoji)]
    }
    return name === undefined ? "" : name
}

/// Code point at `i`, joined when a surrogate pair.
function codeAt(text, i) {
    var high = text.charCodeAt(i)
    if (high >= 0xD800 && high <= 0xDBFF && i + 1 < text.length) {
        var low = text.charCodeAt(i + 1)
        if (low >= 0xDC00 && low <= 0xDFFF) {
            return (high - 0xD800) * 0x400 + (low - 0xDC00) + 0x10000
        }
    }
    return high
}

function isRegional(point) {
    return point >= 0x1F1E6 && point <= 0x1F1FF
}

/// End index of emoji starting at `i` (selectors, skin tones, keycap, flag, ZWJ
/// joins), or `i`.
function emojiEnd(text, i) {
    var point = codeAt(text, i)
    var first = point > 0xFFFF ? 2 : 1
    var next = i + first < text.length ? codeAt(text, i + first) : 0
    if (point < 0x1F000 || point > 0x1FAFF) {
        // A digit, # or * is an emoji only as a keycap.
        var keycap = next === 0xFE0F || next === 0x20E3
        if (point > 0xFFFF || !(Pictures.bmpStarts[point] === true && (point >= 0xA9 || keycap))) {
            return i
        }
    }
    var end = i + first
    if (isRegional(point)) {
        return isRegional(next) ? end + 2 : end
    }
    while (end < text.length) {
        var more = codeAt(text, end)
        var step = more > 0xFFFF ? 2 : 1
        if (more === 0xFE0F || more === 0x20E3
                || (more >= 0x1F3FB && more <= 0x1F3FF)
                || (more >= 0xE0020 && more <= 0xE007F)) {
            end += step
        } else if (more === 0x200D && end + 1 < text.length) {
            end += 1 + (codeAt(text, end + 1) > 0xFFFF ? 2 : 1)
        } else {
            break
        }
    }
    return end
}

/// Message run as StyledText: escaped, breaks and space runs kept.
function escaped(text, lineStart) {
    var out = text.replace(/\r/g, "").replace(/\t/g, " ")
        .replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        .replace(/ {2,}/g, function(run) {
            return " " + new Array(run.length).join("&nbsp;")
        })
        .replace(/\n /g, "\n&nbsp;")
    if (lineStart && out.charAt(0) === " ") {
        out = "&nbsp;" + out.substring(1)
    }
    return out.replace(/\n/g, "<br>")
}

/// `text` with each shipped emoji as `<img>`, as StyledText; "" if none to
/// draw.
///
/// `styled`: text already StyledText (shim's Markdown, sender chars escaped, so
/// "<" only opens shim tags; tags left untouched). Else escaped here. Only tag
/// added: `<img>` under qml/art/emoji/, so a message can load nothing.
///
/// `size`: font pixel size; pictures drawn a bit taller, as font emoji are.
function inText(text, styled, size) {
    // Most messages have no emoji; per-char scan is costly on long bodies.
    if (!text || !Pictures.mayHold.test(text)) {
        return ""
    }
    var known = Pictures.byKey
    var textStyle = Pictures.textStyle
    var edge = Math.round(size * 1.15)
    var out = ""
    var from = 0
    var drew = false
    var inTag = false
    var i = 0
    while (i < text.length) {
        var code = text.charCodeAt(i)
        if (styled && (inTag || code === 0x3C)) {
            inTag = code !== 0x3E
            i++
            continue
        }
        var end = code < 0x23 ? i : emojiEnd(text, i)
        if (end === i) {
            i++
            continue
        }
        var emoji = text.substring(i, end)
        var bare = key(emoji)
        var name = known[bare]
        if (name === undefined) {
            name = known[untoned(emoji)]
        }
        if (name === undefined || (textStyle[bare] === true && bare === emoji)) {
            i = end
            continue
        }
        var before = text.substring(from, i)
        out += styled ? before : escaped(before, from === 0)
        out += "<img src=\"" + artBase + name + ".png\" width=\"" + edge
               + "\" height=\"" + edge + "\" align=\"middle\">"
        drew = true
        from = end
        i = end
    }
    if (!drew) {
        return ""
    }
    var rest = text.substring(from)
    return out + (styled ? rest : escaped(rest, from === 0))
}

/// Pictures' path for `<img>`. art/ is a sibling of js/, components/ and pages/,
/// so it resolves the same whichever file Qt resolves against.
var artBase = Qt.resolvedUrl("../art/emoji/")

/// Recent-emoji cap.
var recentLimit = 40

/// `recent` with `emoji` first, cut to `recentLimit`. New array: dconf needs a
/// new value.
function remember(recent, emoji) {
    var out = [emoji]
    var count = recent ? recent.length : 0
    for (var i = 0; i < count && out.length < recentLimit; i++) {
        if (key(recent[i]) !== key(emoji)) {
            out.push("" + recent[i])
        }
    }
    return out
}
