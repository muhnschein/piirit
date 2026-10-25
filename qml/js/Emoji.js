// Emoji picture lookup, and emoji scan of messages.
//
// Qt 5.6 has no colour fonts: emoji with a shipped Twemoji picture draw as one,
// else text (components/EmojiGlyph.qml). Generated: scripts/fetch-emoji.py.
//
// Library: tables read once. Drawing uses only prebuilt EmojiPictures.js: Qt
// 5.6 keeps no compiled scripts, full list (~3x) slowed first chat each launch.
//
// Tables: `byKey` picture per `key`; `bmpStarts` first char of BMP emoji (scan
// stops only there and at astral chars); `textStyle` text-default symbols
// (c), arrow, check box: emoji only with U+FE0F. Symbols group only: bare
// heart, smiley, sun meant as emoji, drawn as one; star, check mark bare are
// emoji by Unicode, data spells them bare (scripts/fetch-emoji.py `spelled`).
// BMP only: above it a bare sign is an empty box in fonts, so picture.
// `mayHold` key chars (first, or keycap mark) worth scanning for.
.pragma library
.import "EmojiPictures.js" as Pictures

/// Emoji without variation selectors (U+FE0F): comes with or without.
function key(emoji) {
    return emoji.replace(/️/g, "")
}

/// Same, with skin tones removed too.
function untoned(emoji) {
    return key(emoji).replace(/\uD83C[\uDFFB-\uDFFF]/g, "")
}

/// Picture name under qml/art/emoji/ (no extension), or "". Skin tones not
/// shipped: drawn untoned.
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

/// `text` with each shipped emoji as `<img>`, as StyledText; "" if none.
/// `styled`: already StyledText (shim's Markdown, sender chars escaped, tags
/// untouched); else escaped here. Only added tag: `<img>` of picture under
/// qml/art/emoji/, so a message can load nothing. `size`: font px; pictures a
/// bit taller. `tint` (colour, e.g. label's): pictures grey, shaded toward it,
/// as faint, by app's picture provider (`image://piirit/emoji`, shim's
/// pictures.rs); "" or none: as drawn.
///
/// Two passes (cuts, then text). One pass set "text left off" and "scan pos"
/// from one value: Sailfish's ARM64 JIT could clobber it between the two
/// copies, so some launches showed a leading emoji twice (picture + char).
/// Each pass sets each variable from its own value.
function inText(text, styled, size, tint) {
    var cuts = emojiCuts(text, styled)
    if (cuts.length === 0) {
        return ""
    }
    var edge = Math.round(size * 1.15)
    // StyledText reads attribute raw: `&` stays `&`.
    var muted = tint ? "&tint=" + encodeURIComponent("" + tint) + "&strength=" + tintStrength
                     : ""
    var out = ""
    var from = 0
    for (var k = 0; k < cuts.length; k += 3) {
        var before = text.substring(from, cuts[k])
        var file = artBase + cuts[k + 2] + ".png"
        out += styled ? before : escaped(before, from === 0)
        out += "<img src=\"" + (muted ? "image://piirit/emoji?file="
                                       + encodeURIComponent(file) + muted : file)
               + "\" width=\"" + edge
               + "\" height=\"" + edge + "\" align=\"middle\">"
        from = cuts[k + 1]
    }
    var rest = text.substring(from)
    return out + (styled ? rest : escaped(rest, from === 0))
}

/// Label's StyledText: `marked` (Marks.js, or "") else plain `text`, with
/// pictures (`inText`); "" when neither: draw `text` plain.
function drawn(text, marked, size, tint) {
    var styled = marked.length > 0
    var pictured = inText(styled ? marked : text, styled, size, tint)
    return pictured.length > 0 ? pictured : marked
}

/// Emoji `inText` draws: start, end, picture of each, flat list.
function emojiCuts(text, styled) {
    var cuts = []
    // Most messages have none; per-char scan cost ~1 ms of row's frame on long bodies.
    if (!text || !Pictures.mayHold.test(text)) {
        return cuts
    }
    var known = Pictures.byKey
    var textStyle = Pictures.textStyle
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
        // Not where the sender asked for text with U+FE0E, either: a
        // star so marked is the font's. Down here only, as `textStyle` is.
        if (name !== undefined && !(textStyle[bare] === true && bare === emoji)
                && !(code < 0xD800 && text.charCodeAt(end) === 0xFE0E)) {
            cuts.push(i, end, name)
        }
        i = end
    }
    return cuts
}

/// Pictures' path for `<img>`. art/ is sibling of js/, components/, pages/:
/// same from any.
var artBase = Qt.resolvedUrl("../art/emoji/")

/// Tinted pictures' way from grey toward tint: half keeps faces readable on
/// dark and light ambience both.
var tintStrength = 0.5

/// Names (chat, contact, sender) tinted to own colour too: else full colour.
/// Previews always tinted: beside secondary text, colour pulls eye off names.
var tintNames = false

/// `tint` for a name drawn in `color`: "" unless `tintNames`.
function nameTint(color) {
    return tintNames ? color : ""
}

/// Recent-emoji cap.
var recentLimit = 40

/// `recent` with `emoji` first, cut to `recentLimit`. New array: dconf needs it.
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
