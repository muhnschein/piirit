// Time and delivery-mark helpers, shared so lists agree. No `.pragma library`:
// qsTr() needs importing component's context.

/// Elapsed time, fewest chars ("10 min" over "14:32"); past a week: date. `now`
/// from `Clock.now` (bindings never re-run on `Date.now()`); later of it and
/// real clock, which lags a tick.
function timeLabel(seconds, now) {
    if (seconds <= 0) {
        return ""
    }
    var when = new Date(seconds * 1000)
    var elapsed = Math.max(now, Date.now()) - when.getTime()
    if (elapsed < 60000) {
        return qsTr("now")
    }
    if (elapsed < 3600000) {
        return qsTr("%1 min").arg(Math.floor(elapsed / 60000))
    }
    if (elapsed < 86400000) {
        return qsTr("%1 h").arg(Math.floor(elapsed / 3600000))
    }
    if (elapsed < 7 * 86400000) {
        // %n: plural forms. English catalog words "day(s)"; fallback for
        // untranslated languages (else raw source).
        //: How long ago a chat's last message was. %n is a count of days.
        return qsTr("%n day(s)", "", Math.floor(elapsed / 86400000))
    }
    return Qt.formatDate(when, Qt.DefaultLocaleShortDate)
}

/// Human size: decimal units, whole bytes, else one decimal ("1.5 MB").
function readableSize(bytes) {
    if (bytes <= 0) return ""
    var units = ["B", "kB", "MB", "GB"]
    var step = 0
    var size = bytes
    while (size >= 1000 && step < units.length - 1) {
        size = size / 1000
        step++
    }
    return (step === 0 ? Math.round(size) : size.toFixed(1)) + " " + units[step]
}

/// Mark on own sent message from DC_STATE_*: 20 pending, 24 failed, 26
/// delivered, 28 read. None if incoming.
function stateMark(state) {
    if (state === 28) return "✓✓"
    if (state === 26) return "✓"
    if (state === 24) return "✗"
    if (state === 20) return "…"
    return ""
}
