import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Where a tap asked the camera to focus: the platform camera's ring, white
 * while the lens is searching and lit once it has locked. It stays while
 * the tap's focus is held, and goes with it.
 */
Rectangle {
    id: reticle

    /// Where the tap landed, in the parent's coordinates.
    property point at
    /// The camera says it has found focus.
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
