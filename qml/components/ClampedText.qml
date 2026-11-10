import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Someone's own words (status, group description), two lines until tapped: long ones
 * no longer push the page down. Plain text, as written.
 */
Label {
    id: root

    property bool expanded: false

    x: Theme.horizontalPageMargin
    width: parent ? parent.width - 2 * Theme.horizontalPageMargin : 0
    wrapMode: Text.Wrap
    textFormat: Text.PlainText
    truncationMode: TruncationMode.Elide
    maximumLineCount: root.expanded ? 1000 : 2

    MouseArea {
        objectName: "expandTap"
        anchors.fill: parent
        enabled: root.truncated || root.expanded
        onClicked: root.expanded = !root.expanded
    }
}
