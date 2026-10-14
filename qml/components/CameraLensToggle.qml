import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Viewfinder.js" as Viewfinder

/*
 * Back cameras, one ring each, lit = in use. Lens toggle. Shown
 * if 2+ back cameras. Beside shutter (portrait), column (landscape).
 *
 * Ring shows lens zoom ("1.0", "0.5") from labels phone adaptation gives the
 * platform camera; QtMultimedia knows only side and id.
 * No labels: numbered in platform order.
 */
Grid {
    id: toggle

    /// Ids from Viewfinder.camerasFacing.
    property var cameras: []
    property string current: ""
    /// Platform label per camera.
    property var labels: []
    property bool vertical: false

    signal selected(string deviceId)

    columns: vertical ? 1 : Math.max(1, cameras.length)
    rows: vertical ? Math.max(1, cameras.length) : 1
    spacing: Theme.paddingMedium
    visible: cameras.length > 1

    Repeater {
        model: toggle.cameras

        MouseArea {
            objectName: "lens" + index
            width: Theme.itemSizeExtraSmall
            height: width
            readonly property bool lit: modelData === toggle.current || (pressed && containsMouse)
            onClicked: toggle.selected(modelData)

            Rectangle {
                anchors.fill: parent
                radius: width / 2
                color: Qt.rgba(0, 0, 0, 0.3)
                border.width: 2
                border.color: parent.lit ? Theme.highlightColor : "white"
            }

            Label {
                objectName: "lensLabel" + index
                anchors.centerIn: parent
                color: parent.lit ? Theme.highlightColor : "white"
                font.pixelSize: Theme.fontSizeSmall
                text: Viewfinder.lensLabel(toggle.labels, index)
            }
        }
    }
}
