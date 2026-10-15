// Call labels for chat row and Calls page.
//
// No `.pragma library`: qsTr() needs importing file's context.

/// Call row first line: kind or outcome, Delta Chat's words (its translations). `state`:
/// Alerting, Active, Completed, Missed, Declined, Canceled.
function title(state, hasVideo) {
    if (state === "Missed") {
        return qsTr("Missed call")
    }
    if (state === "Declined") {
        return qsTr("Declined call")
    }
    if (state === "Canceled") {
        return qsTr("Canceled call")
    }
    return hasVideo ? qsTr("Video call") : qsTr("Audio call")
}

/// Second line: length, or ringing; none if never connected.
function detail(state, duration, outgoing) {
    if (state === "Completed") {
        if (duration < 60) {
            //: How long a call lasted, when it was shorter than a minute.
            return qsTr("Less than 1 minute")
        }
        //: How long a call lasted. %n is whole minutes.
        return qsTr("%n minute(s) duration", "", Math.floor(duration / 60))
    }
    if (state === "Alerting") {
        return outgoing ? qsTr("Ringing…") : qsTr("Incoming call")
    }
    return ""
}

/// Call that didn't happen: warning colour.
function failed(state) {
    return state === "Missed" || state === "Declined" || state === "Canceled"
}
