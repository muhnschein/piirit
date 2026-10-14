// Which picture draws an emoji, and where the emoji in a message are.
//
// Sailfish draws few emoji itself: Qt 5.6 has no colour fonts, and the
// system fonts cover a few hundred in monochrome at best. So every emoji
// a reaction shows is a Twemoji picture where one is shipped, and text
// only where none is (components/EmojiGlyph.qml). The list of what is
// shipped is generated; see scripts/fetch-emoji.py.
//
// A shared library: nothing here translates, and the lookup tables are
// read once rather than once per row.
//
// Drawing needs only the tables in EmojiPictures.js, which the script
// builds out of the picker's list ahead of time. They used to be built
// here, out of the list itself -- every emoji's search words with it,
// nearly three times the size -- on the first chat opened after every
// launch, since Qt 5.6 keeps no compiled copy of a script. The list is
// the picker's alone now (EmojiPicker.js).
//
// What the tables are, by the names they have there:
//
// - `byKey`: every emoji's picture, by `key`.
// - `bmpStarts`: the first character of every emoji that starts in the
//   Basic Multilingual Plane, by its code: the only ones a scan of a
//   message has to stop at, besides the planes above it.
// - `textStyle`: the symbols that are text unless U+FE0F says otherwise
//   -- a heart, a copyright sign, an arrow -- by key. Unicode draws these
//   as text by default, and so does every other client; a message that
//   says "(c)" with the sign has not sent an emoji. One character and the
//   selector in the data -- fully qualified only with it, so text without
//   it. A star or a check mark is an emoji bare, and the data spells it
//   bare (scripts/fetch-emoji.py, `spelled`). Only in the Basic
//   Multilingual Plane, among the symbols every font has: above it a
//   dove or a sun behind a cloud sent bare would be an empty box, so it
//   is a picture.
// - `mayHold`: matches a text only if it has a character of some emoji's
//   key in it -- the first, or a keycap's mark -- so any text it does not
//   match has nothing in it to draw, and need not be read through.
.pragma library
.import "EmojiPictures.js" as Pictures

/// An emoji with its variation selectors taken out: the same emoji
/// arrives with U+FE0F from one client and without it from another.
function key(emoji) {
    return emoji.replace(/️/g, "")
}

/// The same, with skin tones taken out too.
function untoned(emoji) {
    return key(emoji).replace(/\uD83C[\uDFFB-\uDFFF]/g, "")
}

/// The picture for an emoji, as its file name under qml/art/emoji/
/// without the extension, or "" when none is shipped.
///
/// Skin tones are not shipped (the picker offers none), so a toned
/// reaction from another client is drawn untoned: a thumbs-up in the
/// wrong shade reads better than the empty box Sailfish would draw.
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

/// The code point at `i`, put back together when it is a surrogate pair.
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

/// Where the emoji that may start at `i` ends, or `i` when nothing that
/// could be one starts there. One emoji is its first character and all
/// that hangs off it: selectors, skin tones, a keycap, a flag's tags,
/// and whatever a zero-width joiner joins on; or two regional
/// indicators, which make a flag.
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

/// A run of a message as StyledText shows it as written: escaped, with
/// its line breaks and its runs of spaces kept, which StyledText would
/// otherwise fold into one.
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

/// `text` with every emoji a picture is shipped for drawn as that
/// picture, as StyledText, or "" when there is none to draw and the
/// text is best shown the way it was.
///
/// `styled` says `text` is StyledText already: the shim's rendering of
/// the Markdown, in which every character the sender wrote is escaped,
/// so a "<" can only open one of the shim's own tags and nothing inside
/// one is touched. Otherwise it is the message as written, and is
/// escaped here. Either way the only tag added is an <img> of a picture
/// under qml/art/emoji/, so nothing a message says can load anything.
///
/// `size` is the font's pixel size; the pictures are drawn a little
/// taller than its letters, as an emoji in a font is.
function inText(text, styled, size) {
    // Most messages have no emoji, and the scan below calls out for
    // nearly every character: a long body cost a millisecond of the
    // frame its row was built in.
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
        // Text, too, where the sender asked for it with U+FE0E: a star
        // so marked is the font's. Down here only, as `textStyle` is.
        if (name === undefined || (textStyle[bare] === true && bare === emoji)
                || (code < 0xD800 && text.charCodeAt(end) === 0xFE0E)) {
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

/// Where the pictures are, for an <img> in text. Beside js/ as beside
/// components/ and pages/, so it is the same folder whichever file Qt
/// resolves it against.
var artBase = Qt.resolvedUrl("../art/emoji/")

/// How many emoji the picker remembers having been picked.
var recentLimit = 40

/// `recent` with `emoji` put first and any earlier copy of it taken out,
/// cut to `recentLimit`. A new list rather than the old one changed:
/// it is written back to dconf, which wants a new value to notice.
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
