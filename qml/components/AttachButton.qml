import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * "+" beside message field; opens tray of choices above it.
 *
 * Shape from Whisperfish (qml/components/ChatTextInput.qml). Choices appear under the thumb that opened them.
 *
 * Choices, nearest thumb first: camera (take now); paper clip (platform picker over all indexed media and documents, not one entry per kind); mic (voice message, only where recording works); apps grid (send webxdc, only if reader enabled).
 *
 * Nothing opened here: owning page pushes pickers and starts recording, as ConversationPage does for forwarding. Keeps this loadable and testable alone. Page also closes tray on tap elsewhere.
 */
Item {
    id: root

    /// Tray showing.
    property bool open: false
    /// Mic offered at all, else hidden.
    property bool voiceAvailable: false
    /// Apps grid offered at all. Page binds from reader setting.
    property bool appsAvailable: false
    /// Take picture or video now.
    signal cameraRequested()
    /// Indexed picture, video, song or document.
    signal libraryRequested()
    /// Record voice message now.
    signal voiceRequested()
    /// .xdc app file off phone, to play in chat.
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
        // Animated via plain property: Behavior can't attach to a grouped
        // property member like `icon.rotation`.
        property real turn: root.open ? 45 : 0
        Behavior on turn { NumberAnimation { duration: 150 } }
        icon.rotation: toggle.turn
        onClicked: root.open = !root.open
    }

    // Drawn outside bounds, above button. Nothing in input row clips; row
    // declared after message list, paints over conversation.
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

        // `enabled` follows button, not fade: tray on way out stops taking taps
        // at once. `visible` follows fade: gone, not transparent.
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
