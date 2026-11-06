import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Speech.js" as Speech

/*
 * Voice message's transcript under its player: text, state line, one button. Plain
 * properties: parent binds them from `Transcript` and `Transcriber`; tests set each.
 * Draws nothing until file is on phone. Bindings, not Column: see AttachmentPreview.
 */
Item {
    id: root

    /// `Transcript.state`: 0 none, 1 queued, 2 running, 3 done, 4 no speech,
    /// 5 other language, 6 failed.
    property int transcriptState: 0
    /// Queued or run by a tap.
    property bool manual: false
    /// Running: 0..1000.
    property int permille: 0
    /// Done: transcript; running: text so far. From the sender's voice: plain.
    property string text: ""
    /// Done: spoken language; other language: detected one. Whisper code.
    property string language: ""
    /// Voice file on phone.
    property bool hasFile: false
    /// `Transcriber.model_state`, `model_permille`.
    property int modelState: 0
    property int modelPermille: 0
    /// Battery saving mode on; charger in.
    property bool powerSaving: false
    property bool charging: false
    /// Auto-transcribe: 0 never, 1 while charging, 2 always.
    property int mode: 0
    /// Phone's language, whisper code: footer names only another.
    property string phoneLanguage: Speech.codeOf(Qt.locale().name)
    /// Audio list: folds to three lines, tap unfolds. Chat: six, "Show all".
    property bool compact: false

    /// Button: queue now; `anyLanguage` skips language check.
    signal transcribeRequested(bool anyLanguage)
    /// Running line tapped: stop.
    signal stopRequested()

    /// Drawn state (States board): hidden, ready, charger, saving, waiting, model,
    /// running, done, other, nospeech, failed.
    readonly property string drawn: {
        if (!root.hasFile) {
            return "hidden"
        }
        switch (root.transcriptState) {
        case 0:
            return "ready"
        case 1:
            if (root.modelState === 1) {
                return "model"
            }
            if (root.manual) {
                return "waiting"
            }
            if (root.powerSaving) {
                return "saving"
            }
            // Mode 0: only backlog queues on its own, and backlog waits for charger.
            if (root.mode !== 2 && !root.charging) {
                return "charger"
            }
            return "waiting"
        case 2:
            return "running"
        case 3:
            return "done"
        case 4:
            return "nospeech"
        case 5:
            return "other"
        case 6:
            return "failed"
        default:
            return "hidden"
        }
    }

    /// Unfolded by tap.
    property bool unfolded: false
    readonly property int foldLines: root.compact ? 3 : 6

    readonly property bool showsText: (root.drawn === "done" || root.drawn === "running")
                                      && root.text.length > 0

    /// Detected language's own name.
    readonly property string languageName:
        Speech.nameOf(root.language, Qt.locale(root.language).nativeLanguageName)

    readonly property string status: {
        switch (root.drawn) {
        case "charger":
            return qsTr("This voice message is transcribed when the phone is charging.")
        case "saving":
            return qsTr("Transcription is paused while battery saving mode is on.")
        case "waiting":
            return qsTr("Waiting for other voice messages to be transcribed.")
        case "model":
            //: %1 is a percentage of the speech model downloaded so far.
            return qsTr("Downloading the speech model… %1%")
                .arg(Math.floor(root.modelPermille / 10))
        case "running":
            //: %1 is how much of the voice message is transcribed, as a percentage.
            return qsTr("Transcribing… %1%").arg(Math.floor(root.permille / 10))
        case "other":
            //: %1 is a language's own name, such as "Svenska".
            return qsTr("This voice message seems to be in %1, which is not selected.")
                .arg(root.languageName)
        case "nospeech":
            return qsTr("No speech was found in this voice message.")
        case "failed":
            return qsTr("This voice message could not be transcribed.")
        default:
            return ""
        }
    }

    readonly property string buttonText: {
        switch (root.drawn) {
        case "ready":
            //: Button on a voice message: turns it into text on the phone.
            return qsTr("Transcribe")
        case "charger":
        case "saving":
            return qsTr("Transcribe now")
        case "other":
            // Only if that language could be picked.
            return Speech.isAvailable(root.language) ? qsTr("Transcribe anyway") : ""
        case "failed":
            return qsTr("Try again")
        default:
            return ""
        }
    }

    readonly property string footer: {
        if (root.drawn !== "done") {
            return ""
        }
        //: Under a voice message's transcript: it was made on the phone itself.
        var made = qsTr("Transcribed on this phone")
        return root.language.length > 0 && root.language !== root.phoneLanguage
               ? made + " · " + root.languageName : made
    }

    visible: root.drawn !== "hidden"
    height: root.drawn === "hidden" ? 0 : footerLabel.y + footerLabel.height

    /// Button pressed.
    function act() {
        if (root.drawn === "other") {
            root.transcribeRequested(true)
        } else if (root.buttonText.length > 0) {
            root.transcribeRequested(false)
        }
    }

    // Folded transcript; in Audio list a tap unfolds.
    BackgroundItem {
        id: textArea
        objectName: "transcriptTextArea"
        y: Theme.paddingSmall
        width: parent.width
        visible: root.showsText
        enabled: root.compact && transcriptText.truncated && !root.unfolded
        height: root.showsText ? transcriptText.height : 0
        onClicked: root.unfolded = true

        Label {
            id: transcriptText
            objectName: "transcriptText"
            width: parent.width
            wrapMode: Text.Wrap
            truncationMode: TruncationMode.Elide
            maximumLineCount: root.unfolded ? 100000 : root.foldLines
            color: root.drawn === "running" ? Theme.secondaryColor : Theme.primaryColor
            // Words of the sender's voice: never markup.
            textFormat: Text.PlainText
            text: root.showsText ? root.text : ""
        }
    }

    BackgroundItem {
        id: showAll
        objectName: "transcriptShowAll"
        readonly property bool shown: root.showsText && !root.compact && !root.unfolded
                                      && transcriptText.truncated
        y: textArea.y + textArea.height
        width: parent.width
        visible: showAll.shown
        height: showAll.shown ? showAllLabel.implicitHeight + Theme.paddingSmall : 0
        onClicked: root.unfolded = true

        Label {
            id: showAllLabel
            anchors.bottom: parent.bottom
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.highlightColor
            //: Under a long transcript cut short: shows the rest.
            text: qsTr("Show all")
        }
    }

    // State line. Running: tap stops.
    BackgroundItem {
        id: statusArea
        objectName: "transcriptStatusArea"
        readonly property bool shown: root.status.length > 0
        y: showAll.y + showAll.height + (statusArea.shown ? Theme.paddingSmall : 0)
        width: parent.width
        visible: statusArea.shown
        enabled: root.drawn === "running"
        height: statusArea.shown ? statusLabel.height : 0
        onClicked: root.stopRequested()

        BusyIndicator {
            id: spinner
            objectName: "transcriptBusy"
            size: BusyIndicatorSize.ExtraSmall
            running: root.drawn === "running"
            visible: spinner.running
        }

        Label {
            id: statusLabel
            objectName: "transcriptStatus"
            x: spinner.running ? spinner.width + Theme.paddingSmall : 0
            width: parent.width - x
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeExtraSmall
            color: root.drawn === "failed" ? Theme.errorColor
                   : root.drawn === "model" || root.drawn === "running"
                     ? Theme.highlightColor : Theme.secondaryColor
            textFormat: Text.PlainText
            text: root.status
        }
    }

    // Model download progress.
    Rectangle {
        id: bar
        objectName: "transcriptModelBar"
        readonly property bool shown: root.drawn === "model"
        y: statusArea.y + statusArea.height + (bar.shown ? Theme.paddingSmall : 0)
        width: parent.width
        visible: bar.shown
        height: bar.shown ? 3 : 0
        color: Theme.rgba(Theme.highlightColor, 0.3)

        Rectangle {
            height: parent.height
            width: parent.width * Math.min(1, root.modelPermille / 1000)
            color: Theme.highlightColor
        }
    }

    Button {
        id: button
        objectName: "transcriptButton"
        readonly property bool shown: root.buttonText.length > 0
        y: bar.y + bar.height + (button.shown ? Theme.paddingSmall : 0)
        visible: button.shown
        height: button.shown ? implicitHeight : 0
        text: root.buttonText
        onClicked: root.act()
    }

    Label {
        id: footerLabel
        objectName: "transcriptFooter"
        readonly property bool shown: root.footer.length > 0
        y: button.y + button.height + (footerLabel.shown ? Theme.paddingSmall : 0)
        width: parent.width
        visible: footerLabel.shown
        height: footerLabel.shown ? implicitHeight : 0
        truncationMode: TruncationMode.Fade
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: Text.PlainText
        text: root.footer
    }
}
