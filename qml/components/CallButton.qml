import QtQuick 2.0
import Sailfish.Silica 1.0

// Silica Button, icon and word drawn here: Silica centres the icon's box, whose empty left
// edge pushes what shows right. Small icon; icon and word centred on what shows.
Button {
    id: button

    /// Theme icon, such as "image://theme/icon-m-call".
    property string iconSource
    property string label
    /// Glyph's empty left edge less word's trailing edge, halved: measured on the phone.
    property real inkShift: 0

    Row {
        objectName: "callButtonContent"
        anchors.centerIn: parent
        anchors.horizontalCenterOffset: -button.inkShift
        spacing: Theme.paddingSmall

        Image {
            id: glyph
            objectName: "callButtonIcon"
            anchors.verticalCenter: parent.verticalCenter
            sourceSize: Qt.size(Theme.iconSizeSmallPlus, Theme.iconSizeSmallPlus)
            source: button.iconSource + (button.highlighted ? "?" + Theme.highlightColor : "")
        }
        Label {
            objectName: "callButtonLabel"
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, button.width - glyph.width - Theme.paddingSmall
                                               - 2 * Theme.paddingMedium)
            truncationMode: TruncationMode.Fade
            color: button.highlighted ? Theme.highlightColor : Theme.primaryColor
            text: button.label
        }
    }
}
