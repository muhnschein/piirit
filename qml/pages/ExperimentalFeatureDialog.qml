import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Turning on an experimental feature, asked about once each time. What
 * the feature does is said under its switch; what is said here is the
 * one thing worth stopping for, so it is said on the way in rather than
 * every time the settings page is read. The settings page connects to
 * `accepted` and turns the feature on; turning it off asks nothing.
 */
Dialog {
    id: dialog

    /// The switch's own label, for the header.
    property string title

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
            text: qsTr("This feature may be unstable and may be changed or removed.")
        }
    }
}
