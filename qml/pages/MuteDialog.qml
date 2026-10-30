import QtQuick 2.0
import Sailfish.Silica 1.0

// Nothing preset: blind swipe mutes nothing. No title: accept text says it.
Dialog {
    id: dialog

    property bool muted: false
    // -1 forever, 0 unmute. Meaningful once `chosen`.
    property int seconds: 0
    property bool chosen: false
    // Core's `notify_mentions`: replies still notify. Caller decides.
    property bool mentionsStillNotify: false

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

        // Silica switch self-flips; dialog decides.
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
