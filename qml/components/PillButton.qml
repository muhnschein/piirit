import QtQuick 2.0
import Sailfish.Silica 1.0

/// A word in a pill, with an optional icon before it: a button for the
/// chat list's search row, or a filter that is on or off.
///
/// The tinted backing is what says it can be tapped; while the filter is
/// on it fills with the highlight, the way a pressed Silica control does,
/// and the word takes the highlight colour. A word says what it does where
/// an icon alone would leave it to be guessed.
Item {
    id: root

    /// What the pill says.
    property alias text: label.text
    /// A theme icon shown before the word, as `image://theme/icon-m-…`.
    property string icon: ""
    /// The filter is on. A pill that is only a button stays off.
    property bool checked: false
    /// Being pressed.
    readonly property bool down: mouse.pressed && mouse.containsMouse
    signal clicked()

    readonly property bool lit: root.checked || root.down
    readonly property color ink: root.lit ? Theme.highlightColor : Theme.primaryColor

    // An icon carries its own clear space, so the side it sits on needs
    // less padding to look even with the other.
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
        // The icon's own clear space alone leaves the word crowding it.
        spacing: root.icon !== "" ? Theme.paddingSmall : 0

        Image {
            visible: root.icon !== ""
            anchors.verticalCenter: parent.verticalCenter
            width: Theme.iconSizeSmallPlus
            height: Theme.iconSizeSmallPlus
            sourceSize.width: Theme.iconSizeSmallPlus
            sourceSize.height: Theme.iconSizeSmallPlus
            // The theme draws its icons in whatever colour is asked for.
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
        // A finger is bigger than the pill drawn.
        anchors.margins: -Theme.paddingSmall
        onClicked: root.clicked()
    }
}
