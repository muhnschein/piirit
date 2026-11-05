import QtQuick 2.0
import Sailfish.Silica 1.0
// Qt 5.6: Settings singleton needs this dir imported (see QuickActionSetting).
import "."
import "../js/Transcription.js" as Transcription

/*
 * When voice messages transcribe on their own: choice and backlog switch. Setup's second
 * dialog and transcription page both; each control writes its setting on tap.
 */
Column {
    id: when

    width: parent ? parent.width : 0

    /// Stored choice, unknown as never.
    readonly property int mode: Settings.transcriptionMode === 1 || Settings.transcriptionMode === 2
                                ? Settings.transcriptionMode : 0

    /// Reset shown choice: Silica tap writes currentIndex, detaching binding.
    function refresh() {
        modeCombo.currentIndex = when.mode
    }

    Connections {
        target: Settings
        onTranscriptionModeChanged: when.refresh()
    }

    Component.onCompleted: when.refresh()

    // Label and values as "Auto-download attachments".
    ComboBox {
        id: modeCombo
        objectName: "transcriptionModeCombo"
        width: parent.width
        label: qsTr("Auto-transcribe voice messages")
        description: Transcription.modeDescription(when.mode)

        menu: ContextMenu {
            Repeater {
                model: 3

                MenuItem {
                    objectName: "transcriptionMode" + index
                    text: Transcription.modeName(index)
                    onClicked: Settings.transcriptionMode = index
                }
            }
        }
    }

    TextSwitch {
        objectName: "transcriptionOlderSwitch"
        text: qsTr("Transcribe older voice messages")
        description: qsTr("Voice messages that arrived before you turned this on are transcribed while the phone is charging.")
        automaticCheck: false
        checked: Settings.transcriptionOlder === true
        onClicked: Settings.transcriptionOlder = !checked
    }
}
