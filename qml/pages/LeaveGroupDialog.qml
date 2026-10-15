import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Leave-group confirm. Own page: no way back unless re-added, countdown easy
 * to miss. Group page handles `accepted`.
 */
Dialog {
    id: dialog

    /// Group to leave.
    property string groupName

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        DialogHeader {
            title: qsTr("Leave group")
            acceptText: qsTr("Leave")
            cancelText: qsTr("Cancel")
        }

        Label {
            objectName: "leaveText"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            color: Theme.highlightColor
            // Name: whatever members chose.
            textFormat: Text.PlainText
            //: %1 is the group's name.
            text: qsTr("Leave %1? Only a member can add you back.").arg(dialog.groupName)
        }
    }
}
