import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The back cameras, one ring each, the one in use lit: the platform
 * camera's lens toggle. Shown only on a phone with more than one camera
 * on the back.
 *
 * The rings are numbered in the order the platform lists the cameras.
 * The platform's own app labels them with their zoom ("0.6", "2.0") from
 * a list each device's adaptation writes into that app's settings, which
 * is not this app's to read; what QtMultimedia says about a camera is its
 * side and an id, and nothing about its lens.
 */
Row {
    id: toggle

    /// The device ids, as Viewfinder.camerasFacing lists them.
    property var cameras: []
    property string current: ""

    signal selected(string deviceId)

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
                anchors.centerIn: parent
                color: parent.lit ? Theme.highlightColor : "white"
                font.pixelSize: Theme.fontSizeSmall
                text: index + 1
            }
        }
    }
}
