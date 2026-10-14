// What the reaction picker offers: every emoji, in Unicode's groups, and
// which of them a search finds.
//
// Apart from Emoji.js, which draws them, because this needs the whole of
// EmojiData.js -- every emoji's name and search words -- and drawing does
// not: a chat is opened far more often than the picker, and Qt 5.6 parses
// a script afresh on every launch. Only the picker imports this.
.pragma library
.import "EmojiData.js" as Data

/// The picker's groups, in the order EmojiData.js numbers them, each
/// with the emoji its jump button shows.
var groupIcons = ["😀", "👋", "🐻", "🍔", "🚗", "⚽", "💡", "❤️", "🏁"]

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
