import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The mark for an account: the outline of a head and shoulders inside a
 * ring, which is what every phone draws where a person's picture would
 * go before there is one.
 *
 * Drawn rather than named, for AppMark's reason: the theme has an icon
 * for most things and this app asks it only for names it has been seen
 * to have, and the one place this mark stands is the tile that starts
 * the app -- a tile whose icon is not there is a reader who cannot
 * start. Three rings cost nothing, scale with whatever size they are
 * given, and take the colour of whatever they are put in.
 *
 * The shoulders are the top of a ring the bottom of which is cut off,
 * which `clip` can do because the cut is a straight line.
 */
Item {
    id: root

    /// How big the whole mark is, ring and all.
    property real size: Theme.iconSizeMedium
    /// What colour to draw the lines in.
    property color color: Theme.primaryColor

    /// How thick every line is.
    readonly property real stroke: Math.max(1, Math.round(root.size / 14))

    width: root.size
    height: root.size

    // The ring.
    Rectangle {
        objectName: "accountRing"
        anchors.fill: parent
        radius: width / 2
        color: "transparent"
        border.width: root.stroke
        border.color: root.color
    }

    // The head: a small circle, a little above the middle.
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

    // The shoulders: the upper part of a wider circle, cut off where the
    // ring's inner edge is, so the figure sits in the ring rather than
    // on it.
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
