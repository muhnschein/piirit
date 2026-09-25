// What a call is called, wherever one is listed: its row in the chat,
// and the contact's Calls page. Shared, so the two cannot come to say
// different things about the same call.
//
// No `.pragma library`, for the reason Format.js gives: qsTr() translates
// in the context of the file it is written in, and a shared library has
// none.

/// What a call row says first: what kind of call, or what became of it.
/// Delta Chat's own words, so its translations apply. `state` is the
/// core's name for where the call stands: Alerting, Active, Completed,
/// Missed, Declined or Canceled.
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

/// The line under it: how long a call lasted, or that one is still
/// ringing. Nothing for a call that never connected, whose title already
/// says so.
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

/// A call that did not happen, drawn in the colour that says so.
function failed(state) {
    return state === "Missed" || state === "Declined" || state === "Canceled"
}
