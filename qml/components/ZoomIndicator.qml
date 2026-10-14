import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Viewfinder.js" as Viewfinder

/*
 * Zoom line with dot, shown while zooming and 2 s after. Ends: 1 and camera
 * max. Numbers, not "min"/"max" (translation).
 */
Item {
    id: indicator

    property real zoom: 1
    property real maximumZoom: 1

    /// Shown while running, fades on stop.
    readonly property bool shown: hold.running

    function show() {
        hold.restart()
    }

    implicitWidth: line.width
    implicitHeight: Theme.itemSizeSmall + Theme.paddingLarge
    opacity: shown ? 1 : 0
    visible: opacity > 0
    Behavior on opacity { NumberAnimation { duration: 200 } }

    onZoomChanged: show()

    Timer {
        id: hold
        objectName: "zoomHold"
        interval: 2000
    }

    Label {
        objectName: "zoomValue"
        anchors {
            horizontalCenter: dot.horizontalCenter
            bottom: line.top
            bottomMargin: Theme.paddingSmall
        }
        color: Theme.highlightColor
        font.pixelSize: Theme.fontSizeSmall
        font.bold: true
        text: Viewfinder.zoomText(indicator.zoom)
    }

    Rectangle {
        id: line
        anchors.centerIn: parent
        width: Screen.width * 0.75
        height: 2
        radius: 1
        color: Theme.highlightColor
    }

    Rectangle {
        id: dot
        objectName: "zoomDot"
        anchors.verticalCenter: line.verticalCenter
        x: line.x + line.width * Viewfinder.zoomFraction(indicator.zoom, indicator.maximumZoom)
           - width / 2
        width: Theme.paddingMedium + Theme.paddingSmall
        height: width
        radius: width / 2
        color: Theme.highlightColor
    }

    Label {
        anchors {
            horizontalCenter: line.left
            top: line.bottom
            topMargin: Theme.paddingSmall
        }
        color: Theme.highlightColor
        font.pixelSize: Theme.fontSizeExtraSmall
        font.bold: true
        text: Viewfinder.zoomText(1)
    }

    Label {
        objectName: "zoomMaximum"
        anchors {
            horizontalCenter: line.right
            top: line.bottom
            topMargin: Theme.paddingSmall
        }
        color: Theme.highlightColor
        font.pixelSize: Theme.fontSizeExtraSmall
        font.bold: true
        text: Viewfinder.zoomText(Math.max(1, indicator.maximumZoom))
    }
}
