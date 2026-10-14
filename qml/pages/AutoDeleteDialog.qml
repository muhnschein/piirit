import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Deleting old messages, asked once, on own page (as deltachat-android/ios).
 * Period deletes older messages in every chat of every profile, read or not,
 * now and from then on. Switch must be on before accept. Settings page writes
 * on `accepted`; back edge writes nothing.
 */
Dialog {
    id: dialog

    /// Picked period, seconds, and settings page's label (choice now off screen).
    property int seconds: 0
    property string periodLabel: ""
    /// Messages going now, all profiles: core's count.
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

        // Off until reader turns it; accept disabled till then (reference
        // clients' checkbox).
        TextSwitch {
            id: confirm
            objectName: "confirmSwitch"
            text: qsTr("I understand, delete all these messages")
        }
    }
}
