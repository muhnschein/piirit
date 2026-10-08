import QtQuick 2.0
import Sailfish.Silica 1.0

/// A filter that is on or off: its name in a pill.
///
/// The tinted backing is what says it can be tapped, beside a search field
/// that has none; while the filter is on it fills with the highlight, the
/// way a pressed Silica control does, and the name takes the highlight
/// colour. A word says what it filters where an icon would leave it to be
/// guessed.
Item {
    id: root

    /// The filter's name, shown in the pill.
    property alias text: label.text
    /// The filter is on.
    property bool checked: false
    /// Being pressed.
    readonly property bool down: mouse.pressed && mouse.containsMouse
    signal clicked()

    readonly property bool lit: root.checked || root.down

    width: label.width + 2 * Theme.paddingLarge
    height: label.height + 2 * Theme.paddingMedium

    Rectangle {
        objectName: "filterBacking"
        anchors.fill: parent
        radius: height / 2
        color: root.lit
               ? Theme.rgba(Theme.highlightBackgroundColor,
                            root.down ? Theme.highlightBackgroundOpacity * 1.5
                                      : Theme.highlightBackgroundOpacity)
               : Theme.rgba(Theme.primaryColor, 0.1)
    }

    Label {
        id: label
        objectName: "filterLabel"
        anchors.centerIn: parent
        font.pixelSize: Theme.fontSizeMedium
        color: root.lit ? Theme.highlightColor : Theme.primaryColor
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        // A finger is bigger than the pill drawn.
        anchors.margins: -Theme.paddingSmall
        onClicked: root.clicked()
    }
}
