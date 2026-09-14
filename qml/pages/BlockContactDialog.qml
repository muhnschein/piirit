import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Blocking somebody, or letting them back in, asked about once. Both
 * ways round in one dialog because the question is the same shape and
 * the two are each other's undo.
 *
 * The words are the reference clients' own -- deltachat-android's
 * `menu_block_contact`, `menu_unblock_contact`, `ask_block_contact` and
 * `ask_unblock_contact` -- down to what the blocking one says about
 * groups, which is the part a reader would otherwise have to find out by
 * being surprised. Who it is about is the name on its own line above the
 * question: a name is nobody's to translate.
 *
 * A page of its own rather than a countdown on the row: the row is what
 * the reader tapped, and on the blocked list it goes away the moment the
 * block does. The page that pushed this connects to `accepted` and does
 * the blocking.
 */
Dialog {
    id: dialog

    /// Who the question is about, as the list shows them.
    property string contactName
    /// Which way it goes: blocking, or unblocking.
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
            // The name is the contact's own, or whatever they were
            // called here.
            textFormat: Text.PlainText
            text: dialog.contactName
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
