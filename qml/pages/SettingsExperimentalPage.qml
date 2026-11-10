import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Speech.js" as Speech

/*
 * Features that may change or go. From SettingsPage. Each switch off at once;
 * on after reader accepts risk. Transcription's own settings under its switch
 * while on.
 */
Page {
    id: page

    /// Unused: every settings page takes it (SettingsPage pushes it).
    property int accountId

    /// App-wide `Transcriber`; null (tests, loaded alone): no model size in setup,
    /// download state unknown.
    property QtObject transcriber: typeof appWindow !== "undefined" ? appWindow.transcriber : null

    /// `MODEL_FAILED` (transcripts.rs).
    readonly property bool modelFailed: page.transcriber !== null
                                        && page.transcriber.model_state === 3

    /// Switch: off at once; on after reader accepts risk. `next` (`url`, `properties`):
    /// dialog replacing question on accept, which turns setting on itself.
    function toggleExperimental(toggle, setting, next) {
        if (toggle.checked) {
            Settings[setting] = false
            return
        }
        var properties = { title: toggle.text }
        if (next) {
            properties.acceptDestination = next.url
            properties.acceptDestinationProperties = next.properties
            properties.acceptDestinationAction = PageStackAction.Replace
        }
        var dialog = pageStack.push(Qt.resolvedUrl("ExperimentalFeatureDialog.qml"), properties)
        if (dialog && !next) {
            dialog.accepted.connect(function() {
                Settings[setting] = true
            })
        }
    }

    /// Transcription switch: question, then languages (which turn it on), then timing.
    function toggleTranscription() {
        page.toggleExperimental(transcriptionSwitch, "transcriptionEnabled", {
            url: Qt.resolvedUrl("TranscriptionLanguagesDialog.qml"),
            properties: { setup: true, returnTo: page, transcriber: page.transcriber }
        })
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Experimental")
            }

            // As Quick actions' intro sentence.
            Label {
                objectName: "experimentalExplained"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("These features may be unstable, changed or removed.")
            }

            // Off until asked: only place "webxdc" shows on fresh phone. Gates
            // tray entry, store, running apps; each reads setting.
            TextSwitch {
                id: webxdcSwitch
                objectName: "webxdcSwitch"
                //: A webxdc app is a small program somebody sends into a
                //: chat and everyone in it plays with. Keep the name:
                //: it is what every other Delta Chat client calls them.
                text: qsTr("Webxdc apps")
                description: qsTr("Runs small apps inside chats.")
                automaticCheck: false
                checked: Settings.webxdcEnabled === true
                onClicked: page.toggleExperimental(webxdcSwitch, "webxdcEnabled")
            }

            // Off until asked, as reference clients. Gates tray entry, map tile, all-chats map,
            // map from rows; sharing already on still shows its bar, so it can be stopped.
            TextSwitch {
                id: locationSwitch
                objectName: "locationSwitch"
                //: Delta Chat's name for sharing your location in a chat, once or live.
                text: qsTr("Location streaming")
                description: qsTr("Shares your location in chats and shows shared locations on a map.")
                automaticCheck: false
                checked: Settings.locationStreamingEnabled === true
                onClicked: page.toggleExperimental(locationSwitch, "locationStreamingEnabled")
            }

            SectionHeader {
                //: Section of the experimental settings: voice messages turned into text.
                text: qsTr("Transcription")
            }

            // On: languages dialog writes it (TranscriptionLanguagesDialog.qml).
            TextSwitch {
                id: transcriptionSwitch
                objectName: "transcriptionSwitch"
                text: qsTr("Transcribe voice messages")
                // One sentence per string (docs/PROJECT.md).
                description: qsTr("Turns voice messages into text on this phone.") + " "
                             + qsTr("Nothing leaves the phone.")
                automaticCheck: false
                checked: Settings.transcriptionEnabled === true
                onClicked: page.toggleTranscription()
            }

            // Set up: its settings under switch, as setup asked them.
            Column {
                objectName: "transcriptionSettings"
                visible: Settings.transcriptionEnabled === true
                width: parent.width
                spacing: Theme.paddingMedium

                ValueButton {
                    objectName: "languagesEntry"
                    width: parent.width
                    label: qsTr("Languages")
                    value: Speech.names(Settings.transcriptionLanguages)
                    onClicked: pageStack.push(Qt.resolvedUrl("TranscriptionLanguagesDialog.qml"))
                }

                TranscriptionWhen {}

                Label {
                    objectName: "modelFailed"
                    visible: page.modelFailed
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    wrapMode: Text.Wrap
                    color: Theme.errorColor
                    text: qsTr("The speech model could not be downloaded.")
                }

                Button {
                    objectName: "modelRetry"
                    visible: page.modelFailed
                    anchors.horizontalCenter: parent.horizontalCenter
                    //: Starts the speech model download again.
                    text: qsTr("Try again")
                    onClicked: page.transcriber.download_model()
                }
            }
        }

        VerticalScrollDecorator {}
    }
}
