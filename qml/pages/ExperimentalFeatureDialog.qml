import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Asked each time an experimental feature turns on; off asks nothing.
 * Settings connects `accepted`; other switches set `text` (QuickActionSetting).
 */
Dialog {
    id: dialog

    /// Switch label.
    property string title
    /// The one stop-worthy thing.
    property string text: qsTr("This feature may be unstable and may be changed or removed.")

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        DialogHeader {
            title: dialog.title
            //: Accepts turning on an experimental feature.
            acceptText: qsTr("Enable")
            cancelText: qsTr("Cancel")
        }

        Label {
            objectName: "experimentalText"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            color: Theme.highlightColor
            text: dialog.text
        }
    }
}
