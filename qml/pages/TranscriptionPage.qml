import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Speech.js" as Speech
import "../js/Format.js" as Format

/*
 * Transcription once set up, from entry under Settings switch: languages, timing (as setup),
 * speech model and transcripts on phone. Delete all waits on Silica's countdown first.
 */
Page {
    id: page

    /// App-wide `Transcriber`; null (tests, loaded alone): storage unknown.
    property QtObject transcriber: typeof appWindow !== "undefined" ? appWindow.transcriber : null

    /// `MODEL_DOWNLOADING`, `MODEL_FAILED` (transcripts.rs).
    readonly property bool downloading: page.transcriber !== null
                                        && page.transcriber.model_state === 1
    readonly property bool failed: page.transcriber !== null
                                   && page.transcriber.model_state === 3
    readonly property int transcripts: page.transcriber !== null
                                       ? page.transcriber.transcript_count : 0

    /// Speech model line: name, then download, size on phone, or nothing.
    readonly property string modelText: {
        if (page.transcriber === null) {
            return ""
        }
        var name = page.transcriber.model_name
        if (page.downloading) {
            //: %1 is a percentage of the speech model downloaded so far.
            return name + " · " + qsTr("Downloading the speech model… %1%")
                .arg(Math.floor(page.transcriber.model_permille / 10))
        }
        if (page.transcriber.model_bytes > 0) {
            //: %1 is a size, such as "190 MB".
            return name + " · " + qsTr("%1 on this phone")
                .arg(Format.readableSize(page.transcriber.model_bytes))
        }
        return name
    }

    function openLanguages() {
        pageStack.push(Qt.resolvedUrl("TranscriptionLanguagesDialog.qml"), {
            transcriber: page.transcriber
        })
    }

    function deleteAll() {
        //: What Silica's countdown says it is doing, before every transcript is deleted.
        remorse.execute(qsTr("Deleting all transcripts"), function () {
            page.transcriber.delete_all()
        })
    }

    RemorsePopup {
        id: remorse
        objectName: "deleteRemorse"
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

            SectionHeader {
                //: Heading over what transcription keeps on the phone.
                text: qsTr("Storage")
            }

            Column {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin

                Label {
                    width: parent.width
                    text: qsTr("Speech model")
                }
                Label {
                    objectName: "modelLabel"
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: page.failed ? Theme.errorColor : Theme.secondaryColor
                    text: page.failed ? qsTr("The speech model could not be downloaded.")
                                      : page.modelText
                }
            }

            Button {
                objectName: "modelRetry"
                visible: page.failed
                anchors.horizontalCenter: parent.horizontalCenter
                //: Starts the speech model download again.
                text: qsTr("Try again")
                onClicked: page.transcriber.download_model()
            }

            Column {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin

                Label {
                    width: parent.width
                    text: qsTr("Transcripts")
                }
                Label {
                    objectName: "transcriptsLabel"
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    //: How many voice messages have a transcript on the phone.
                    text: qsTr("%n voice message(s)", "", page.transcripts)
                          + (page.transcripts > 0 && page.transcriber !== null
                             ? " · " + Format.readableSize(page.transcriber.transcript_bytes)
                             : "")
                }
            }

            Button {
                objectName: "deleteAllButton"
                anchors.horizontalCenter: parent.horizontalCenter
                enabled: page.transcripts > 0
                text: qsTr("Delete all transcripts")
                onClicked: page.deleteAll()
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                // One sentence per string (docs/PROJECT.md).
                text: qsTr("Voice messages are transcribed on this phone.") + " "
                      + qsTr("Neither the voice messages nor their transcripts leave the phone, and transcripts are not part of a backup.")
            }
        }

        VerticalScrollDecorator {}
    }
}
