import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Viewfinder.js" as Viewfinder

/*
 * Camera settings, platform camera's way: viewfinder-top icons show what's in
 * force; tap pulls panel down, column per setting (timer, flash, white
 * balance, grid); tap elsewhere hides. Page sets choices (`flashModes` empty
 * on front camera), decides what picks (signals) mean.
 */
Item {
    id: panel

    property bool open: false
    /// Top offset: under notch, if any.
    property real topInset: 0

    property var flashModes: []
    property int flash: Viewfinder.flashOff
    property int timer: 0
    property int whiteBalance: Viewfinder.whiteBalanceAuto
    property bool grid: false

    signal flashChosen(int mode)
    signal timerChosen(int seconds)
    signal whiteBalanceChosen(int mode)
    signal gridChosen(bool on)

    /// Header text while choice pressed, and a moment after.
    property string described: ""

    function flashName(mode) {
        switch (mode) {
        case Viewfinder.flashAuto: return qsTr("Flash automatic")
        case Viewfinder.flashOn: return qsTr("Flash on")
        case Viewfinder.flashTorch: return qsTr("Flashlight on")
        default: return qsTr("Flash off")
        }
    }

    function timerName(seconds) {
        return seconds > 0 ? qsTr("Self-timer %1 s").arg(seconds) : qsTr("Self-timer off")
    }

    function whiteBalanceName(mode) {
        switch (mode) {
        case Viewfinder.whiteBalanceSunlight: return qsTr("Sunny")
        case Viewfinder.whiteBalanceCloudy: return qsTr("Cloudy")
        case Viewfinder.whiteBalanceFluorescent: return qsTr("Fluorescent")
        case Viewfinder.whiteBalanceTungsten: return qsTr("Tungsten")
        default: return qsTr("Automatic white balance")
        }
    }

    function gridName(on) {
        return on ? qsTr("Grid on") : qsTr("Grid off")
    }

    function names(values, name) {
        var out = []
        for (var i = 0; i < values.length; i++) {
            out.push(name(values[i]))
        }
        return out
    }

    function icons(values, icon) {
        var out = []
        for (var i = 0; i < values.length; i++) {
            out.push(icon(values[i]))
        }
        return out
    }

    function describe(text) {
        panel.described = text
        describedHold.restart()
    }

    onOpenChanged: if (!open) panel.described = ""

    Timer {
        id: describedHold
        interval: 3000
        onTriggered: panel.described = ""
    }

    // Behind panel: tap hides it.
    MouseArea {
        objectName: "settingsDismiss"
        anchors.fill: parent
        enabled: panel.open
        onClicked: panel.open = false
    }

    // In-force icons; tap opens panel.
    MouseArea {
        id: topRow
        objectName: "settingsRow"
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        height: panel.topInset + Theme.itemSizeSmall
        enabled: !panel.open
        opacity: panel.open ? 0 : 1
        Behavior on opacity { NumberAnimation { duration: 200 } }
        onClicked: panel.open = true

        Row {
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: parent.bottom
            }
            spacing: Theme.paddingLarge

            Image {
                objectName: "flashIndicator"
                visible: panel.flashModes.length > 0
                source: Viewfinder.flashIcon(panel.flash) + "?#ffffff"
            }
            Image {
                objectName: "timerIndicator"
                source: Viewfinder.timerIcon(panel.timer) + "?#ffffff"
            }
            Image {
                visible: panel.whiteBalance !== Viewfinder.whiteBalanceAuto
                source: Viewfinder.whiteBalanceIcon(panel.whiteBalance) + "?#ffffff"
            }
            Image {
                source: Viewfinder.gridIcon(panel.grid) + "?#ffffff"
            }
        }
    }

    Item {
        id: sheet
        objectName: "settingsSheet"
        width: parent.width
        height: panel.topInset + header.height + columns.height + 2 * Theme.paddingLarge
        y: panel.open ? 0 : -height
        visible: y > -height
        Behavior on y { NumberAnimation { duration: 200; easing.type: Easing.InOutQuad } }

        Rectangle {
            anchors.fill: parent
            color: "black"
            opacity: 0.7
        }

        // Sheet taps stay on sheet.
        MouseArea {
            anchors.fill: parent
        }

        Label {
            id: header
            objectName: "settingsHeader"
            anchors {
                top: parent.top
                topMargin: panel.topInset
                horizontalCenter: parent.horizontalCenter
            }
            height: Theme.itemSizeSmall
            verticalAlignment: Text.AlignVCenter
            color: Theme.highlightColor
            font.pixelSize: Theme.fontSizeExtraSmall
            font.bold: true
            text: panel.described
        }

        Row {
            id: columns
            anchors {
                top: header.bottom
                horizontalCenter: parent.horizontalCenter
            }
            spacing: Theme.paddingLarge

            CameraSettingsColumn {
                objectName: "timerColumn"
                values: Viewfinder.timerDelays
                icons: panel.icons(Viewfinder.timerDelays, Viewfinder.timerIcon)
                labels: panel.names(Viewfinder.timerDelays, panel.timerName)
                current: panel.timer
                onChosen: panel.timerChosen(value)
                onDescribed: panel.describe(text)
            }

            CameraSettingsColumn {
                objectName: "flashColumn"
                values: panel.flashModes
                icons: panel.icons(panel.flashModes, Viewfinder.flashIcon)
                labels: panel.names(panel.flashModes, panel.flashName)
                current: panel.flash
                onChosen: panel.flashChosen(value)
                onDescribed: panel.describe(text)
            }

            CameraSettingsColumn {
                objectName: "whiteBalanceColumn"
                values: Viewfinder.whiteBalanceModes
                icons: panel.icons(Viewfinder.whiteBalanceModes, Viewfinder.whiteBalanceIcon)
                labels: panel.names(Viewfinder.whiteBalanceModes, panel.whiteBalanceName)
                current: panel.whiteBalance
                onChosen: panel.whiteBalanceChosen(value)
                onDescribed: panel.describe(text)
            }

            CameraSettingsColumn {
                objectName: "gridColumn"
                values: [false, true]
                icons: [Viewfinder.gridIcon(false), Viewfinder.gridIcon(true)]
                labels: [panel.gridName(false), panel.gridName(true)]
                current: panel.grid
                onChosen: panel.gridChosen(value)
                onDescribed: panel.describe(text)
            }
        }
    }
}
