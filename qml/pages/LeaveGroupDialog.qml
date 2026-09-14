import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Leaving a group, asked about once. There is no way back in without
 * someone else adding you, and a countdown on a pull-down is easy to
 * miss with a thumb already moving -- so this is a page of its own, with
 * the platform's accept and cancel at the top. The group page connects
 * to `accepted` and does the leaving.
 */
Dialog {
    id: dialog

    /// The group about to be left, for the question.
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
            // The group's name is whatever its members chose.
            textFormat: Text.PlainText
            //: %1 is the group's name.
            text: qsTr("Leave %1? Only a member can add you back.").arg(dialog.groupName)
        }
    }
}
