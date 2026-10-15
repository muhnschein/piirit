import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Account mark: head and shoulders in ring, as phones draw before a picture exists. Drawn,
 * not named (see AppMark): theme icon may be missing, and this tile starts app. Rings scale
 * with size, take host colour. Shoulders: ring top, bottom cut straight by `clip`.
 */
Item {
    id: root

    /// Whole mark size, ring included.
    property real size: Theme.iconSizeMedium
    /// Line colour.
    property color color: Theme.primaryColor

    /// Line thickness.
    readonly property real stroke: Math.max(1, Math.round(root.size / 14))

    width: root.size
    height: root.size

    // Ring.
    Rectangle {
        objectName: "accountRing"
        anchors.fill: parent
        radius: width / 2
        color: "transparent"
        border.width: root.stroke
        border.color: root.color
    }

    // Head: small circle, above middle.
    Rectangle {
        objectName: "accountHead"
        width: root.size * 0.3
        height: width
        radius: width / 2
        x: (root.size - width) / 2
        y: root.size * 0.22
        color: "transparent"
        border.width: root.stroke
        border.color: root.color
    }

    // Shoulders: top of wider circle, cut at ring's inner edge: figure sits in ring.
    Item {
        objectName: "accountShoulders"
        clip: true
        width: root.size * 0.62
        height: root.size * 0.22
        x: (root.size - width) / 2
        y: root.size * 0.58

        Rectangle {
            width: parent.width
            height: width
            radius: width / 2
            color: "transparent"
            border.width: root.stroke
            border.color: root.color
        }
    }
}
