import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Speech.js" as Speech

/*
 * Languages voice messages are transcribed in. Setup (after "experimental" question):
 * Next turns feature on (Rust starts model download), opens TranscriptionWhenDialog;
 * cancel leaves it off. Later, from TranscriptionPage: languages only. Phone language
 * first, picked at setup; greyed languages can't be picked.
 */
Dialog {
    id: dialog

    /// First run: Next, feature on, timing dialog follows.
    property bool setup: false
    /// Settings page, where timing dialog's Done returns.
    property Item returnTo: null
    /// App-wide `Transcriber`, for model size; null: size not said.
    property QtObject transcriber: null
    /// Phone's language, whisper code. Tests set.
    property string phoneLanguage: Speech.codeOf(Qt.locale().name)

    /// Picked codes. Reassigned whole: bindings see change.
    property var chosen: []

    /// Rows in shown order.
    readonly property var rows: Speech.ordered(dialog.phoneLanguage)
    readonly property var availableRows: dialog.rows.filter(function (row) { return row.available })
    readonly property var greyedRows: dialog.rows.filter(function (row) { return !row.available })

    /// Picked ones as setting: shown order, comma list.
    function chosenList() {
        return dialog.rows.filter(function (row) {
            return dialog.chosen.indexOf(row.code) >= 0
        }).map(function (row) {
            return row.code
        }).join(",")
    }

    /// Flip one; greyed never.
    function toggle(code) {
        if (!Speech.isAvailable(code)) {
            return
        }
        var at = dialog.chosen.indexOf(code)
        var next = dialog.chosen.slice()
        if (at >= 0) {
            next.splice(at, 1)
        } else {
            next.push(code)
        }
        dialog.chosen = next
    }

    canAccept: dialog.chosen.length > 0

    acceptDestination: dialog.setup ? Qt.resolvedUrl("TranscriptionWhenDialog.qml") : null
    acceptDestinationProperties: ({
        "returnTo": dialog.returnTo,
        "transcriber": dialog.transcriber
    })

    onAccepted: {
        Settings.transcriptionLanguages = dialog.chosenList()
        if (dialog.setup) {
            Settings.transcriptionEnabled = true
        }
    }

    Component.onCompleted: {
        var stored = Speech.split(Settings.transcriptionLanguages).filter(Speech.isAvailable)
        if (stored.length === 0 && Speech.isAvailable(dialog.phoneLanguage)) {
            stored = [dialog.phoneLanguage]
        }
        dialog.chosen = stored
        if (dialog.setup) {
            //: Accepts the languages; the timing choice comes next.
            header.acceptText = qsTr("Next")
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            DialogHeader {
                id: header
            }

            Label {
                objectName: "transcriptionIntro"
                visible: dialog.setup
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                // One sentence per string (docs/PROJECT.md).
                text: qsTr("Piirit downloads a speech model once and transcribes voice messages on this phone.")
                      + " " + qsTr("Nothing you send or receive leaves the phone.")
            }

            SectionHeader {
                //: Heading over the languages voice messages can be transcribed in.
                text: qsTr("Languages")
            }

            Repeater {
                model: dialog.availableRows

                TextSwitch {
                    objectName: "language_" + modelData.code
                    text: modelData.name
                    automaticCheck: false
                    checked: dialog.chosen.indexOf(modelData.code) >= 0
                    onClicked: dialog.toggle(modelData.code)
                }
            }

            SectionHeader {
                //: Heading over languages transcription does not offer yet.
                text: qsTr("Not available yet")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: qsTr("Transcription is not accurate enough in these languages yet.")
            }

            Repeater {
                model: dialog.greyedRows

                TextSwitch {
                    objectName: "language_" + modelData.code
                    enabled: false
                    text: modelData.name
                    automaticCheck: false
                    checked: false
                    onClicked: dialog.toggle(modelData.code)
                }
            }

            Label {
                objectName: "transcriptionModelSize"
                readonly property real megabytes: dialog.transcriber !== null
                    ? Math.round(dialog.transcriber.model_download_bytes / 1000000) : 0
                visible: dialog.setup && megabytes > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                //: %1 is the speech model's size in megabytes, such as 190.
                text: qsTr("The speech model needs %1 MB of storage.").arg(megabytes)
            }
        }
    }
}
