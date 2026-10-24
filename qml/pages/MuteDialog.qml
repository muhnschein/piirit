import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * How long a chat stays muted: reference clients' five, one hour to
 * forever. Unmute needs no dialog: menus do it at once, as theirs. Nothing
 * preset: blind swipe mutes nothing; back edge mutes nothing. Caller
 * connects to `picked`.
 */
Dialog {
    id: dialog

    /// Seconds picked; -1 forever, 0 until picked.
    property int seconds: 0

    /// Reader swiped forward on one.
    signal picked(int seconds)

    // Reference clients' order and values (deltachat-android `MuteDialog`).
    readonly property var choices: [3600, 28800, 86400, 604800, -1]

    canAccept: dialog.seconds !== 0
    onAccepted: dialog.picked(dialog.seconds)

    /// Delta Chat's words, whole: translations come along.
    function labelFor(value) {
        switch (value) {
        case 3600:
            return qsTr("Mute for 1 hour")
        case 28800:
            return qsTr("Mute for 8 hours")
        case 86400:
            return qsTr("Mute for 1 day")
        case 604800:
            return qsTr("Mute for 7 days")
        default:
            return qsTr("Mute forever")
        }
    }

    /// Turn one on, rest off. Tap again clears.
    function choose(value) {
        dialog.seconds = dialog.seconds === value ? 0 : value
    }

    Column {
        width: parent.width

        DialogHeader {
            objectName: "muteHeader"
            //: Title of the page that asks how long to mute a chat.
            title: qsTr("Mute notifications")
            acceptText: qsTr("Mute")
            cancelText: qsTr("Cancel")
        }

        // `automaticCheck: false`: Silica switch self-flips; one answer,
        // dialog decides.
        Repeater {
            model: dialog.choices

            TextSwitch {
                objectName: "muteOption" + modelData
                text: dialog.labelFor(modelData)
                automaticCheck: false
                checked: dialog.seconds === modelData
                onClicked: dialog.choose(modelData)
            }
        }
    }
}
