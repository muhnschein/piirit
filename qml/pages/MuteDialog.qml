import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * How long a chat stays muted: reference clients' five, one hour to
 * forever. Unmute needs no dialog: menus do it at once, as theirs. Nothing
 * preset: blind swipe mutes nothing; back edge mutes nothing. Caller
 * connects to `picked`.
 *
 * No title: accept text says what accepting does. Options say only how
 * long.
 */
Dialog {
    id: dialog

    /// Seconds picked; -1 forever, 0 until picked.
    property int seconds: 0
    /// Group with mention setting on: replies to reader still notify
    /// (ChatList `notify_mentions`). Caller decides; note says so.
    property bool mentionsStillNotify: false

    /// Reader swiped forward on one.
    signal picked(int seconds)

    // Reference clients' order and values (deltachat-android `MuteDialog`).
    readonly property var choices: [3600, 28800, 86400, 604800, -1]

    canAccept: dialog.seconds !== 0
    onAccepted: dialog.picked(dialog.seconds)

    function labelFor(value) {
        switch (value) {
        case 3600:
            //: How long a chat stays muted; one of five choices under the
            //: dialog's Mute button.
            return qsTr("For 1 hour")
        case 28800:
            return qsTr("For 8 hours")
        case 86400:
            return qsTr("For 1 day")
        case 604800:
            return qsTr("For 7 days")
        default:
            return qsTr("Forever")
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

        Item {
            width: 1
            height: Theme.paddingLarge
            visible: dialog.mentionsStillNotify
        }

        // What still gets through, said before muting.
        Label {
            objectName: "mentionsNote"
            visible: dialog.mentionsStillNotify
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            //: Under the mute choices of a group, while "Mentions" is on
            //: in the notification settings.
            text: qsTr("You still get a notification when someone in this group replies to one of your messages.")
        }
    }
}
