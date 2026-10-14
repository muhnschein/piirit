import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Tap focus ring: white while searching, lit when locked. Stays while the tap's
 * focus is held, goes with it.
 */
Rectangle {
    id: reticle

    /// Tap point, parent coords.
    property point at
    /// Camera has focus.
    property bool locked: false

    x: at.x - width / 2
    y: at.y - height / 2
    width: Theme.itemSizeMedium
    height: width
    radius: width / 2
    color: "transparent"
    border.width: 2
    border.color: locked ? Theme.highlightColor : "white"
}
