import QtQuick 2.0
import Sailfish.Silica 1.0

/// A filter that is on or off: a funnel in a round button.
///
/// The round backing is what says it can be tapped, beside a search field
/// that has none; while the filter is on it fills with the highlight, the
/// way a pressed Silica control does, and the funnel takes the highlight
/// colour.
///
/// The funnel is drawn rather than an image: Silica's theme has no such
/// icon, the package carries no SVG, and a Canvas comes back blank after
/// the app has been in the background (QrPage.qml). Six strokes of the
/// theme's own colour need none of that.
Item {
    id: root

    /// The filter is on.
    property bool checked: false
    /// Being pressed.
    readonly property bool down: mouse.pressed && mouse.containsMouse
    signal clicked()

    width: Theme.itemSizeExtraSmall
    height: Theme.itemSizeExtraSmall
    // Still says it is on while it is out of use, only fainter.
    opacity: enabled ? 1.0 : Theme.opacityLow

    readonly property bool lit: root.checked || root.down
    // The funnel on a 24-unit grid, as line icons are drawn.
    readonly property real unit: Theme.iconSizeSmallPlus / 24
    // Thin, like the search field's own magnifier beside it.
    readonly property real stroke: Math.max(2, Math.round(root.unit))

    Rectangle {
        objectName: "filterBacking"
        anchors.fill: parent
        radius: width / 2
        color: root.lit
               ? Theme.rgba(Theme.highlightBackgroundColor,
                            root.down ? Theme.highlightBackgroundOpacity * 1.5
                                      : Theme.highlightBackgroundOpacity)
               : Theme.rgba(Theme.primaryColor, 0.1)
    }

    Item {
        id: glyph
        width: Theme.iconSizeSmallPlus
        height: Theme.iconSizeSmallPlus
        anchors.centerIn: parent

        Repeater {
            // From one end to the other, in grid units: the rim, the two
            // sides narrowing to the neck, and the spout.
            model: [
                [3, 5, 21, 5],
                [3, 5, 10, 12.5],
                [21, 5, 14, 12.5],
                [10, 12.5, 10, 19],
                [10, 19, 14, 19],
                [14, 19, 14, 12.5]
            ]
            Rectangle {
                objectName: "filterStroke"
                readonly property real dx: (modelData[2] - modelData[0]) * root.unit
                readonly property real dy: (modelData[3] - modelData[1]) * root.unit
                // Each stroke is a bar with round ends, a stroke longer
                // than the segment so the ends meet as round joins.
                width: Math.sqrt(dx * dx + dy * dy) + root.stroke
                height: root.stroke
                radius: root.stroke / 2
                antialiasing: true
                color: root.lit ? Theme.highlightColor : Theme.primaryColor
                x: (modelData[0] + modelData[2]) / 2 * root.unit - width / 2
                y: (modelData[1] + modelData[3]) / 2 * root.unit - height / 2
                rotation: Math.atan2(dy, dx) * 180 / Math.PI
            }
        }
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        // A finger is bigger than the circle drawn.
        anchors.margins: -Theme.paddingSmall
        onClicked: root.clicked()
    }
}
