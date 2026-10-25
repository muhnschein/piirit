// Search marks: found text bold in highlight colour, as sibling apps vuo, salama
// draw them (bold alone hard to make out on secondary-colour line).
.pragma library
.import "Emoji.js" as Emoji

/// `text` as StyledText, each `query` occurrence marked; "" when none (draw as
/// written, plain). Whole query, any case: core's match (`LIKE '%query%'`).
/// Text escaped (Emoji.js `escaped`): only added tags = marks, so peer text
/// loads nothing. Emoji.js `inText(marked, true, size)` adds pictures after.
///
/// Lowercase changing length (dotted capital I → two units) leaves positions
/// unmappable: nothing marked. Missing mark beats misplaced one.
function marked(text, query, color) {
    var term = query ? ("" + query).trim().toLowerCase() : ""
    if (!text || term.length === 0) {
        return ""
    }
    var folded = text.toLowerCase()
    if (folded.length !== text.length) {
        return ""
    }
    var at = folded.indexOf(term)
    if (at < 0) {
        return ""
    }
    var open = "<b><font color=\"" + color + "\">"
    var out = ""
    var from = 0
    while (at >= 0) {
        out += Emoji.escaped(text.substring(from, at), from === 0)
        out += open + Emoji.escaped(text.substring(at, at + term.length), at === 0)
               + "</font></b>"
        from = at + term.length
        at = folded.indexOf(term, from)
    }
    return out + Emoji.escaped(text.substring(from), false)
}
