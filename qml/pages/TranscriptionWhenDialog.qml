import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Setup's second dialog, after languages accepted (download already started): download
 * progress on top, when to transcribe. Done returns to Settings; back returns to languages.
 * Download belongs to app-wide `Transcriber`: goes on when this closes.
 */
Dialog {
    id: dialog

    /// Settings page Done returns to; null: pop this only.
    property Item returnTo: null
    /// App-wide `Transcriber`, for progress; null: none shown.
    property QtObject transcriber: null

    /// `MODEL_DOWNLOADING`, `MODEL_FAILED` (transcripts.rs).
    readonly property bool downloading: dialog.transcriber !== null
                                        && dialog.transcriber.model_state === 1
    readonly property bool failed: dialog.transcriber !== null
                                   && dialog.transcriber.model_state === 3

    acceptDestination: dialog.returnTo
    acceptDestinationAction: PageStackAction.Pop

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingSmall

            DialogHeader {
                //: Closes transcription setup.
                acceptText: qsTr("Done")
            }

            Label {
                objectName: "modelProgress"
                visible: dialog.downloading
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.highlightColor
                //: %1 is a percentage of the speech model downloaded so far.
                text: dialog.downloading
                      ? qsTr("Downloading the speech model… %1%")
                        .arg(Math.floor(dialog.transcriber.model_permille / 10))
                      : ""
            }

            Rectangle {
                visible: dialog.downloading
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                height: 3
                color: Theme.rgba(Theme.highlightColor, 0.3)

                Rectangle {
                    height: parent.height
                    width: dialog.downloading
                           ? parent.width * Math.min(1, dialog.transcriber.model_permille / 1000)
                           : 0
                    color: Theme.highlightColor
                }
            }

            Label {
                visible: dialog.downloading
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: qsTr("The download continues after you close this page.")
            }

            Label {
                objectName: "modelFailed"
                visible: dialog.failed
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.errorColor
                text: qsTr("The speech model could not be downloaded.")
            }

            Button {
                objectName: "modelRetry"
                visible: dialog.failed
                anchors.horizontalCenter: parent.horizontalCenter
                //: Starts the speech model download again.
                text: qsTr("Try again")
                onClicked: dialog.transcriber.download_model()
            }

            Item {
                visible: dialog.downloading || dialog.failed
                width: 1
                height: Theme.paddingMedium
            }

            TranscriptionWhen {}
        }
    }
}
