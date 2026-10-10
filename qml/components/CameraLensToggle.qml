import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Viewfinder.js" as Viewfinder

/*
 * The back cameras, one ring each, the one in use lit: the platform
 * camera's lens toggle. Shown only on a phone with more than one camera
 * on the back. A row beside the shutter in portrait, a column in
 * landscape, as the platform camera has it.
 *
 * Each ring wears the lens's zoom ("1.0", "0.5") from the labels the
 * phone's adaptation gives the platform camera, which QtMultimedia has
 * no word for: what it says about a camera is its side and an id. A
 * phone whose adaptation gives none gets the rings numbered in the order
 * the platform lists the cameras.
 */
Grid {
    id: toggle

    /// The device ids, as Viewfinder.camerasFacing lists them.
    property var cameras: []
    property string current: ""
    /// The platform's label for each, in the same order.
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
