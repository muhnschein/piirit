import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * App mark: nine squares in a grid, the phone "apps" mark.
 *
 * Drawn, not named: app asks theme only for icon names seen on devices (missing icon = invisible button), and no webxdc icon name can be relied on. Rects scale with size, take host colour.
 *
 * Bindings, not Grid: positioner sizes in a polish pass, which never runs headless. Mark would measure zero in tests.
 */
Item {
    id: root

    /// Whole mark size. Squares divide it: three squares, two gaps.
    property real size: Theme.iconSizeSmall
    /// Colour.
    property color color: Theme.primaryColor

    readonly property real cell: root.size / 4
    readonly property real gap: root.size / 8

    width: root.size
    height: root.size

    Repeater {
        model: 9

        Rectangle {
            width: root.cell
            height: root.cell
            radius: root.cell / 4
            x: (index % 3) * (root.cell + root.gap)
            y: Math.floor(index / 3) * (root.cell + root.gap)
            color: root.color
        }
    }
}
