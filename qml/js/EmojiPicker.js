// Reaction picker: emoji in Unicode groups, and search. Not in Emoji.js: needs
// all of EmojiData.js (names, keywords), drawing doesn't, chats open far more
// than the picker, and Qt 5.6 reparses a script each launch. Picker-only import.
.pragma library
.import "EmojiData.js" as Data

/// Groups in EmojiData.js order, with jump-button emoji.
var groupIcons = ["😀", "👋", "🐻", "🍔", "🚗", "⚽", "💡", "❤️", "🏁"]

/// All emoji as [emoji, picture, group].
function all() {
    return Data.list
}

/// Emoji with each word of `query` at a name or keyword start ("hot": hot
/// pepper, not screenshot). Lowercased. [] for a query with no words.
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

/// `list` cut into `columns`-wide rows. Group start gets a heading row if
/// `headings`.
///
/// Row: {heading: "", group, cells: [[emoji, picture], ...]} or
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
