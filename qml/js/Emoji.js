// Which picture draws an emoji, and which emoji a search finds.
//
// Sailfish draws few emoji itself: Qt 5.6 has no colour fonts, and the
// system fonts cover a few hundred in monochrome at best. So every emoji
// a reaction shows is a Twemoji picture where one is shipped, and text
// only where none is (components/EmojiGlyph.qml). The list of what is
// shipped is generated; see scripts/fetch-emoji.py.
//
// A shared library: nothing here translates, and the lookup table is
// built once rather than once per row.
.pragma library
.import "EmojiData.js" as Data

/// The picker's groups, in the order EmojiData.js numbers them, each
/// with the emoji its jump button shows.
var groupIcons = ["😀", "👋", "🐻", "🍔", "🚗", "⚽", "💡", "❤️", "🏁"]

var byKey = null

/// An emoji with its variation selectors taken out: the same emoji
/// arrives with U+FE0F from one client and without it from another.
function key(emoji) {
    return emoji.replace(/️/g, "")
}

/// The same, with skin tones taken out too.
function untoned(emoji) {
    return key(emoji).replace(/\uD83C[\uDFFB-\uDFFF]/g, "")
}

function table() {
    if (byKey === null) {
        byKey = {}
        for (var i = 0; i < Data.list.length; i++) {
            byKey[key(Data.list[i][0])] = Data.list[i][1]
        }
    }
    return byKey
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
    var known = table()
    var name = known[key(emoji)]
    if (name === undefined) {
        name = known[untoned(emoji)]
    }
    return name === undefined ? "" : name
}

/// Every emoji on offer, as [emoji, picture, group] entries in order.
function all() {
    return Data.list
}

/// The emoji whose name or keywords hold every word of `query`, each as
/// the start of one of them: "hot" finds the hot pepper, not a
/// screenshot. Lowercase; [] for a query with no words in it.
function search(query) {
    var words = query.toLowerCase().split(/\s+/).filter(function(word) {
        return word.length > 0
    })
    if (words.length === 0) {
        return []
    }
    var found = []
    for (var i = 0; i < Data.list.length; i++) {
        var text = " " + Data.list[i][3]
        var matches = true
        for (var w = 0; w < words.length && matches; w++) {
            matches = text.indexOf(" " + words[w]) >= 0
        }
        if (matches) {
            found.push(Data.list[i])
        }
    }
    return found
}

/// `list` cut into rows of `columns`, for a view whose rows are cells
/// laid side by side. A group starting puts its heading first, as a row
/// of its own, when `headings` is given: one title per group number.
///
/// Each row is {heading: "", group, cells: [[emoji, picture], ...]} or
/// {heading: title, group, cells: []}.
function rows(list, columns, headings) {
    var out = []
    var current = null
    var group = -1
    for (var i = 0; i < list.length; i++) {
        var entry = list[i]
        if (headings && entry[2] !== group) {
            group = entry[2]
            out.push({ heading: headings[group], group: group, cells: [] })
            current = null
        }
        if (current === null || current.cells.length >= columns) {
            current = { heading: "", group: entry[2], cells: [] }
            out.push(current)
        }
        current.cells.push([entry[0], entry[1]])
    }
    return out
}

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
