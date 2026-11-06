import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Speech.js" as Speech

/*
 * Transcription once set up, from entry under Settings switch: languages, timing (as setup).
 * Failed model download says so here, with Try again.
 */
Page {
    id: page

    /// App-wide `Transcriber`; null (tests, loaded alone): download state unknown.
    property QtObject transcriber: typeof appWindow !== "undefined" ? appWindow.transcriber : null

    /// `MODEL_FAILED` (transcripts.rs).
    readonly property bool failed: page.transcriber !== null
                                   && page.transcriber.model_state === 3

    function openLanguages() {
        pageStack.push(Qt.resolvedUrl("TranscriptionLanguagesDialog.qml"))
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Voice message transcription")
            }

            SectionHeader {
                //: Heading over the languages voice messages are transcribed in.
                text: qsTr("Languages")
            }

            ValueButton {
                objectName: "languagesEntry"
                width: parent.width
                label: qsTr("Languages")
                value: Speech.names(Settings.transcriptionLanguages)
                onClicked: page.openLanguages()
            }

            SectionHeader {
                //: Heading over when voice messages are transcribed.
                text: qsTr("When")
            }

            TranscriptionWhen {}

            Label {
                objectName: "modelFailed"
                visible: page.failed
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.errorColor
                text: qsTr("The speech model could not be downloaded.")
            }

            Button {
                objectName: "modelRetry"
                visible: page.failed
                anchors.horizontalCenter: parent.horizontalCenter
                //: Starts the speech model download again.
                text: qsTr("Try again")
                onClicked: page.transcriber.download_model()
            }
        }

        VerticalScrollDecorator {}
    }
}
