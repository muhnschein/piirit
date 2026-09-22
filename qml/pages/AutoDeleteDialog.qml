import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Deleting old messages, asked about once.
 *
 * Setting a period deletes every message older than it the moment the
 * core hears of it, in every chat of every profile, read or not -- and
 * keeps doing so from then on. That is the one setting a tap should not
 * be enough for, so it is a page of its own, the way deltachat-android
 * and deltachat-ios ask: how many messages go now, what that includes,
 * and a switch the reader has to turn before accept means anything. The
 * settings page connects to `accepted` and writes the setting; a dialog
 * left by the back edge writes nothing.
 */
Dialog {
    id: dialog

    /// The period the reader picked, in seconds, and how the settings
    /// page names it -- said again here, since the choice is off the
    /// screen by now.
    property int seconds: 0
    property string periodLabel: ""
    /// How many messages would go now, across every profile: what the
    /// core answered when asked.
    property int count: 0

    canAccept: confirm.checked

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        DialogHeader {
            title: qsTr("Delete messages from device")
            acceptText: qsTr("Delete")
            cancelText: qsTr("Cancel")
        }

        Label {
            objectName: "periodLabel"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            color: Theme.highlightColor
            textFormat: Text.PlainText
            text: dialog.periodLabel
        }

        Label {
            objectName: "countLabel"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            color: Theme.highlightColor
            textFormat: Text.PlainText
            //: %n is how many messages would be deleted straight away.
            text: qsTr("%n message(s) will be deleted now, and every new message will be deleted once it reaches that age.", "", dialog.count)
        }

        Column {
            width: parent.width
            spacing: Theme.paddingSmall

            Label {
                objectName: "mediaNote"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                textFormat: Text.PlainText
                text: qsTr("This includes pictures, videos and files.")
            }

            Label {
                objectName: "unreadNote"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                textFormat: Text.PlainText
                text: qsTr("Messages are deleted whether they were read or not.")
            }

            Label {
                objectName: "savedNote"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                textFormat: Text.PlainText
                //: "Saved messages" is the name of the chat with oneself.
                text: qsTr("\"Saved messages\" are kept.")
            }
        }

        // Off until the reader turns it: accept does nothing before that,
        // which is the checkbox the reference clients put on this dialog.
        TextSwitch {
            id: confirm
            objectName: "confirmSwitch"
            text: qsTr("I understand, delete all these messages")
        }
    }
}
