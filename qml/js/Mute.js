// Chat mute words: choices, row value, time left. No `.pragma library`: qsTr() needs
// importing file's context. Length: seconds, -1 forever, 0 on. End: Unix seconds.

/// Choices, in reference clients' order (deltachat-android `MuteDialog`), "on" first.
var choices = [0, 3600, 28800, 86400, 604800, -1]

/// "8 hours", "7 days".
function lengthName(seconds) {
    if (seconds % 86400 === 0) {
        //: How long a chat stays muted, in "Off for %1".
        return qsTr("%n day(s)", "", seconds / 86400)
    }
    //: How long a chat stays muted, in "Off for %1".
    return qsTr("%n hour(s)", "", Math.round(seconds / 3600))
}

/// Time left until `until`, two units at most: "5 h 12 min", "6 d 23 h", "40 min".
function timeLeft(until, now) {
    var minutes = Math.max(1, Math.ceil((until - now) / 60))
    var hours = Math.floor(minutes / 60)
    var days = Math.floor(hours / 24)
    if (days > 0) {
        //: Mute time left, days and hours.
        return hours % 24 > 0 ? qsTr("%1 d %2 h").arg(days).arg(hours % 24)
                              : qsTr("%1 d").arg(days)
    }
    if (hours > 0) {
        //: Mute time left, hours and minutes.
        return minutes % 60 > 0 ? qsTr("%1 h %2 min").arg(hours).arg(minutes % 60)
                                : qsTr("%1 h").arg(hours)
    }
    //: Mute time left, minutes.
    return qsTr("%1 min").arg(minutes)
}

/// Choice's line on the notifications page.
function choiceName(seconds) {
    if (seconds === 0) {
        //: First choice on a chat's notifications page, and Notifications row: not muted.
        return qsTr("On")
    }
    if (seconds < 0) {
        //: Last choice on a chat's notifications page: muted until turned on again. Also
        //: Notifications row when muted with no end known.
        return qsTr("Off")
    }
    //: Choice on a chat's notifications page. %1 is how long, e.g. "8 hours".
    return qsTr("Off for %1").arg(lengthName(seconds))
}

/// Same, for the active choice: time left after it when known.
function activeChoiceName(seconds, until, now) {
    if (seconds > 0 && until > now) {
        //: The active choice. %1 is the choice, e.g. "Off for 8 hours";
        //: %2 the time left, e.g. "5 h 12 min".
        return qsTr("%1 (%2 remaining)").arg(choiceName(seconds)).arg(timeLeft(until, now))
    }
    return choiceName(seconds)
}

/// Notifications row value; `short`: without the length, for a narrow row.
function rowValue(muted, seconds, until, now, short) {
    if (!muted || !(seconds > 0 && until > now)) {
        return choiceName(muted ? -1 : 0)
    }
    return short
           //: Notifications row when the full line doesn't fit. %1: time left.
           ? qsTr("Off (%1 remaining)").arg(timeLeft(until, now))
           //: Notifications row. %1: how long, e.g. "8 hours"; %2: time left.
           : qsTr("Off for %1 (%2 remaining)").arg(lengthName(seconds))
                                             .arg(timeLeft(until, now))
}
