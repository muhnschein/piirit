import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * How long a chat stays muted: reference clients' five, one hour to
 * forever. Chat list menu unmutes at once, as theirs; contact/group page
 * row opens this for both, so muted chat gets Unmute first. Nothing preset:
 * blind swipe changes nothing; back edge changes nothing. Caller connects to
 * `picked`.
 *
 * No title: accept text says what accepting does. Options say only how
 * long.
 */
Dialog {
    id: dialog

    /// Chat muted now: Unmute offered (0 seconds).
    property bool muted: false
    /// Seconds picked; -1 forever, 0 unmute. Meaningful once `chosen`.
    property int seconds: 0
    property bool chosen: false
    /// Group with mention setting on: replies to reader still notify
    /// (ChatList `notify_mentions`). Caller decides; note says so.
    property bool mentionsStillNotify: false

    /// Reader swiped forward on one.
    signal picked(int seconds)

    // Reference clients' order and values (deltachat-android `MuteDialog`).
    readonly property var lengths: [3600, 28800, 86400, 604800, -1]
    readonly property var choices: dialog.muted ? [0].concat(dialog.lengths) : dialog.lengths

    canAccept: dialog.chosen
    onAccepted: dialog.picked(dialog.seconds)

    function labelFor(value) {
        switch (value) {
        case 0:
            return qsTr("Unmute")
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
        if (dialog.chosen && dialog.seconds === value) {
            dialog.chosen = false
            return
        }
        dialog.seconds = value
        dialog.chosen = true
    }

    Column {
        width: parent.width

        DialogHeader {
            objectName: "muteHeader"
            acceptText: dialog.chosen && dialog.seconds === 0 ? qsTr("Unmute") : qsTr("Mute")
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
                checked: dialog.chosen && dialog.seconds === modelData
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
