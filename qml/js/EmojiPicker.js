// Reaction picker: Unicode groups, search. Apart from Emoji.js: needs all of
// EmojiData.js, which Qt 5.6 reparses each launch; chats open far more often.
.pragma library
.import "EmojiData.js" as Data

/// Groups in EmojiData.js order, with jump-button emoji.
var groupIcons = ["😀", "👋", "🐻", "🍔", "🚗", "⚽", "💡", "❤️", "🏁"]

/// All emoji as [emoji, picture, group].
function all() {
    return Data.list
}

/// Emoji with each `query` word at name/keyword start ("hot": hot pepper, not
/// screenshot); lowercased; no words: [].
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

/// `list` in `columns`-wide rows; heading row per group if `headings`.
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
