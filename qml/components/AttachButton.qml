import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * "+" beside message field: tray above it, under the thumb (shape: Whisperfish
 * qml/components/ChatTextInput.qml). Nearest first: camera; paper clip (platform
 * picker over all indexed media and documents, not per kind); location; mic (where
 * recording works); apps grid (webxdc, if enabled). Opens nothing: page pushes pickers,
 * records, closes tray on outside tap, so this loads and tests alone.
 */
Item {
    id: root

    /// Tray showing.
    property bool open: false
    /// Mic offered.
    property bool voiceAvailable: false
    /// Apps grid offered; page binds setting.
    property bool appsAvailable: false
    /// Location entry offered; page binds setting.
    property bool locationAvailable: false
    /// Take picture or video now.
    signal cameraRequested()
    /// Indexed picture, video, song or document.
    signal libraryRequested()
    /// Send location, or share it live.
    signal locationRequested()
    /// Record voice message now.
    signal voiceRequested()
    /// .xdc file off phone, for chat.
    signal appRequested()

    function close() {
        root.open = false
    }

    width: toggle.width
    height: toggle.height

    IconButton {
        id: toggle
        objectName: "attachToggle"
        anchors.bottom: parent.bottom
        icon.source: "image://theme/icon-m-add"
        // Plain property: Behavior can't attach to grouped member `icon.rotation`.
        property real turn: root.open ? 45 : 0
        Behavior on turn { NumberAnimation { duration: 150 } }
        icon.rotation: toggle.turn
        onClicked: root.open = !root.open
    }

    // Above button, outside bounds: input row doesn't clip, paints after message list.
    Rectangle {
        id: tray
        objectName: "attachTray"
        anchors {
            horizontalCenter: toggle.horizontalCenter
            bottom: toggle.top
        }
        width: toggle.width
        height: choices.height + 2 * Theme.paddingSmall
        radius: width / 4
        color: Theme.rgba(Theme.highlightDimmerColor, 0.9)

        // `enabled` follows button: fading tray stops taps at once. `visible` follows fade.
        opacity: root.open ? 1.0 : 0.0
        visible: opacity > 0.0
        enabled: root.open
        Behavior on opacity { NumberAnimation { duration: 150 } }

        Column {
            id: choices
            anchors.centerIn: parent
            spacing: Theme.paddingSmall

            IconButton {
                objectName: "attachCamera"
                icon.source: "image://theme/icon-m-camera"
                onClicked: {
                    root.close()
                    root.cameraRequested()
                }
            }

            IconButton {
                objectName: "attachLibrary"
                icon.source: "image://theme/icon-m-attach"
                onClicked: {
                    root.close()
                    root.libraryRequested()
                }
            }

            IconButton {
                objectName: "attachLocation"
                visible: root.locationAvailable
                icon.source: "image://theme/icon-m-location"
                onClicked: {
                    root.close()
                    root.locationRequested()
                }
            }

            IconButton {
                objectName: "attachVoice"
                visible: root.voiceAvailable
                icon.source: "image://theme/icon-m-mic"
                onClicked: {
                    root.close()
                    root.voiceRequested()
                }
            }

            // Drawn, not named: see AppMark.
            BackgroundItem {
                id: appChoice
                objectName: "attachApp"
                visible: root.appsAvailable
                width: toggle.width
                height: toggle.height

                AppMark {
                    anchors.centerIn: parent
                    size: Theme.iconSizeSmall
                    color: appChoice.highlighted ? Theme.highlightColor
                                                 : Theme.primaryColor
                }

                onClicked: {
                    root.close()
                    root.appRequested()
                }
            }
        }
    }
}
