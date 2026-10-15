import QtQuick 2.0
import Sailfish.Silica 1.0

/// Pill: word, optional icon; button or filter. Tint = tappable, checked = filled.
Item {
    id: root

    /// Pill text.
    property alias text: label.text
    /// Theme icon, `image://theme/icon-m-…`.
    property string icon: ""
    /// Filter on.
    property bool checked: false
    /// Pressed.
    readonly property bool down: mouse.pressed && mouse.containsMouse
    signal clicked()

    readonly property bool lit: root.checked || root.down
    readonly property color ink: root.lit ? Theme.highlightColor : Theme.primaryColor

    // Icon's clear space: less padding on its side.
    width: content.width + Theme.paddingLarge
           + (root.icon !== "" ? Theme.paddingMedium : Theme.paddingLarge)
    height: label.height + 2 * Theme.paddingMedium

    Rectangle {
        objectName: "pillBacking"
        anchors.fill: parent
        radius: height / 2
        color: root.lit
               ? Theme.rgba(Theme.highlightBackgroundColor,
                            root.down ? Theme.highlightBackgroundOpacity * 1.5
                                      : Theme.highlightBackgroundOpacity)
               : Theme.rgba(Theme.primaryColor, 0.1)
    }

    Row {
        id: content
        x: root.icon !== "" ? Theme.paddingMedium : Theme.paddingLarge
        anchors.verticalCenter: parent.verticalCenter
        // Icon margin alone crowds word.
        spacing: root.icon !== "" ? Theme.paddingSmall : 0

        Image {
            visible: root.icon !== ""
            anchors.verticalCenter: parent.verticalCenter
            width: Theme.iconSizeSmallPlus
            height: Theme.iconSizeSmallPlus
            sourceSize.width: Theme.iconSizeSmallPlus
            sourceSize.height: Theme.iconSizeSmallPlus
            // Theme icon takes requested colour.
            source: root.icon !== "" ? root.icon + "?" + root.ink : ""
        }

        Label {
            id: label
            objectName: "pillLabel"
            anchors.verticalCenter: parent.verticalCenter
            font.pixelSize: Theme.fontSizeMedium
            color: root.ink
        }
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        // Finger bigger than drawn pill.
        anchors.margins: -Theme.paddingSmall
        onClicked: root.clicked()
    }
}
