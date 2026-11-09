import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Media.js" as Media

/*
 * One-line row on contact and group pages: kind's icon and name; page opens what it
 * names. Kinds as Media.js. AppMark draws apps' icon. Half a line in DoorGrid.
 */
BackgroundItem {
    id: row

    property string kind
    property real leftMargin: Theme.horizontalPageMargin
    property real rightMargin: Theme.horizontalPageMargin
    /// Name's own width, unclipped: DoorGrid checks it fits.
    readonly property real labelWidth: label.implicitWidth

    readonly property color tint: row.highlighted ? Theme.highlightColor : Theme.primaryColor

    height: Theme.itemSizeSmall

    Image {
        id: icon
        x: row.leftMargin
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.iconSizeMedium
        height: width
        source: row.kind === "apps" ? ""
                : "image://theme/" + Media.kindIcon(row.kind) + "?" + row.tint
    }

    // Drawn, not named (see AppMark). 0.6 = icon's fill of its box.
    AppMark {
        objectName: "appMark"
        visible: row.kind === "apps"
        anchors.centerIn: icon
        size: Theme.iconSizeMedium * 0.6
        color: row.tint
    }

    Label {
        id: label
        objectName: "doorLabel"
        x: icon.x + icon.width + Theme.paddingLarge
        width: parent.width - x - row.rightMargin
        anchors.verticalCenter: parent.verticalCenter
        truncationMode: TruncationMode.Fade
        color: row.tint
        text: Media.kindName(row.kind)
    }
}
