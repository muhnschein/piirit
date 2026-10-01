// What the rows that show a time or a delivery mark have in common, so a
// chat reads the same in every list it appears in. Shared rather than
// copied: the two copies of timeLabel had already started to drift.
//
// No `.pragma library`: qsTr() translates in the context of the component
// that imports this, and a shared library has none.

/// How long ago, in as few characters as will say it.
///
/// "10 min" reads faster than "14:32" when the answer wanted is
/// "recently", and it does not need the reader to know what time it is
/// now. Past a week the elapsed count stops meaning anything, so it goes
/// back to a date.
///
/// `now` is the present in milliseconds, and the caller passes
/// `Clock.now` rather than leaving it to be read here: a binding that
/// reads `Date.now()` is never told the time has changed, and kept saying
/// "now" for as long as the row lived (see Clock.qml).
function timeLabel(seconds, now) {
    if (seconds <= 0) {
        return ""
    }
    var when = new Date(seconds * 1000)
    var elapsed = now - when.getTime()
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
        // %n, so a language with three plural forms gets all three. The
        // English catalog is what turns the source's "day(s)" into a
        // word, which is why the app falls back to it for a language it
        // has no catalog for: without one loaded, %n shows the source
        // text as it stands.
        //: How long ago a chat's last message was. %n is a count of days.
        return qsTr("%n day(s)", "", Math.floor(elapsed / 86400000))
    }
    return Qt.formatDate(when, Qt.DefaultLocaleShortDate)
}

/// A size a person can read. Decimal units, as the platform's own file
/// manager uses; whole bytes, one decimal for everything else -- "1.5 MB",
/// not "1.5 B".
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

/// The mark beside a message we sent, from its DC_STATE_*: 20 pending,
/// 24 failed, 26 delivered, 28 read. Nothing for an incoming message.
function stateMark(state) {
    if (state === 28) return "✓✓"
    if (state === 26) return "✓"
    if (state === 24) return "✗"
    if (state === 20) return "…"
    return ""
}
