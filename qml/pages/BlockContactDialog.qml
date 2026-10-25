import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Emoji.js" as Emoji

/*
 * Block or unblock confirm, one dialog both ways (each other's undo). Words
 * from deltachat-android (`menu_block_contact`, `menu_unblock_contact`,
 * `ask_block_contact`, `ask_unblock_contact`), incl. groups note; name on own
 * line (untranslated). Own page: row vanishes once blocked. Pusher handles
 * `accepted`.
 */
Dialog {
    id: dialog

    /// Subject: contact as list shows.
    property string contactName
    /// Direction: block or unblock.
    property bool blocking: true

    readonly property string action: dialog.blocking ? qsTr("Block contact")
                                                     : qsTr("Unblock contact")

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        DialogHeader {
            title: dialog.action
            acceptText: dialog.action
            cancelText: qsTr("Cancel")
        }

        Label {
            objectName: "blockName"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.highlightColor
            // Contact's own name, or given one; emoji as Twemoji (Emoji.js).
            readonly property string pictured: Emoji.inText(dialog.contactName, false,
                                                            font.pixelSize, color, true)
            textFormat: pictured.length > 0 ? Text.StyledText : Text.PlainText
            text: pictured.length > 0 ? pictured : dialog.contactName
        }

        Label {
            objectName: "blockText"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            color: Theme.primaryColor
            text: dialog.blocking
                  ? qsTr("Block this contact?\n\nDirect messages or groups created by blocked contacts will not show up.\n\nOther groups with blocked contacts will still show their messages.")
                  : qsTr("Unblock this contact?")
        }
    }
}
