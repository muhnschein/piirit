import QtQuick 2.0
import Sailfish.Silica 1.0

/// A filter that is on or off, drawn as the three narrowing lines the
/// reference mail and chat clients use for one.
///
/// Drawn rather than an image: Silica's theme has no such icon, and one
/// made of three rectangles takes the theme's colours and sizes with no
/// rendering step and no file per size and ambience.
BackgroundItem {
    id: root

    /// The filter is on.
    property bool checked: false

    width: Theme.itemSizeSmall
    height: Theme.itemSizeSmall
    // Still says it is on while it is out of use, only fainter.
    opacity: enabled ? 1.0 : Theme.opacityLow

    readonly property color lineColor:
        root.checked || root.down ? Theme.highlightColor : Theme.primaryColor
    // On a 24-unit grid, the way such icons are drawn: lines 15, 9 and 3
    // long, 1.5 thick, 5 apart, centred.
    readonly property real unit: Theme.iconSizeMedium / 24

    Repeater {
        model: [15, 9, 3]
        Rectangle {
            objectName: "filterLine"
            width: modelData * root.unit
            height: 1.5 * root.unit
            radius: height / 2
            color: root.lineColor
            x: (root.width - width) / 2
            y: root.height / 2 + (index - 1) * 5 * root.unit - height / 2
        }
    }
}
