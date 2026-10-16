import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Group request turned down: confirm before its messages go (no undo; tap
 * beside Accept easy). Conversation page handles `accepted`.
 */
Dialog {
    id: dialog

    /// Group to delete.
    property string groupName

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        DialogHeader {
            //: Title of the question asked before a group request is deleted.
            title: qsTr("Delete group")
            acceptText: qsTr("Delete")
            cancelText: qsTr("Cancel")
        }

        Label {
            objectName: "deleteGroupText"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            color: Theme.highlightColor
            // Name: whatever members chose.
            textFormat: Text.PlainText
            //: %1 is the group's name.
            text: qsTr("Delete %1 and its messages from this device?").arg(dialog.groupName)
        }
    }
}
