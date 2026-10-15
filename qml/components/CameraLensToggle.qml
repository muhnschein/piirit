import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Viewfinder.js" as Viewfinder

/*
 * Back-lens toggle: ring per camera, lit = in use; if 2+. Ring shows zoom
 * ("1.0", "0.5") from adaptation's platform-camera labels (QtMultimedia knows
 * only side and id); none: numbered in platform order.
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
