import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Viewfinder.js" as Viewfinder

/*
 * Exposure compensation: the platform camera's slider, a thin track
 * standing at the side of the viewfinder with the exposure icon riding
 * on it. Brighter is up. The value shows beside the handle while it is
 * moved and for two seconds after; a double tap puts it back to none.
 *
 * Half a stop at a time from two under to two over, which is the
 * platform camera's range; a camera that cannot go as far clamps it.
 */
Item {
    id: slider

    /// The compensation in EV, one of Viewfinder.exposureSteps.
    property real value: 0
    /// Which side the value is written on.
    property bool labelOnLeft: true

    signal moved(real value)

    readonly property real travel: height - handle.height

    function setFraction(fraction) {
        var next = Viewfinder.exposureAt(fraction)
        if (next !== slider.value) {
            slider.moved(next)
        }
        shown.restart()
    }

    width: Theme.itemSizeMedium

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        y: handle.height / 2
        width: 2
        height: slider.travel
        color: "white"
        opacity: 0.6

        // The mark at none.
        Rectangle {
            anchors.centerIn: parent
            width: Theme.paddingLarge
            height: 2
            color: "white"
        }
    }

    Item {
        id: handle
        objectName: "exposureHandle"
        anchors.horizontalCenter: parent.horizontalCenter
        y: slider.travel * Viewfinder.exposureFraction(slider.value)
        width: Theme.itemSizeExtraSmall
        height: width

        Rectangle {
            anchors.fill: parent
            radius: Theme.paddingSmall
            color: "black"
            opacity: 0.6
        }

        Image {
            anchors.centerIn: parent
            source: "image://theme/icon-camera-exposure-compensation"
                    + (area.pressed ? "?" + Theme.highlightColor : "")
        }
    }

    Label {
        objectName: "exposureValue"
        anchors.verticalCenter: handle.verticalCenter
        x: slider.labelOnLeft ? -width - Theme.paddingSmall : slider.width + Theme.paddingSmall
        color: Theme.highlightColor
        font.bold: true
        text: Viewfinder.exposureText(slider.value)
        opacity: area.pressed || shown.running ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 200 } }
    }

    Timer {
        id: shown
        interval: 2000
    }

    MouseArea {
        id: area
        objectName: "exposureArea"
        anchors.fill: parent
        preventStealing: true
        onPressed: slider.setFraction((mouse.y - handle.height / 2) / Math.max(1, slider.travel))
        onPositionChanged: slider.setFraction((mouse.y - handle.height / 2) / Math.max(1, slider.travel))
        onDoubleClicked: {
            slider.moved(0)
            shown.restart()
        }
    }
}
