import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0

/*
 * Voice recording in place of message field; own component, headless-testable. Recorder
 * here; page gets recording or nothing. Tap start, tap send (not hold: no accidental
 * release). Page hides field and attach button meanwhile. Length capped by relay size
 * (over 1.5 h), unshown: at cap, recording stops and sends.
 */
Item {
    id: root

    /// Recording possible here.
    readonly property bool available: recorder.available
    /// Running or finishing.
    readonly property bool recording: recorder.recording
    /// Relay max bytes (core limit; 0 = unknown). Recording stops at longest fit.
    property real limitBytes: 0
    /// Quality: 0 balanced, 1 less data. Sets bit rate.
    property int mediaQuality: 0
    /// Done; `path` to send.
    signal recorded(string path)
    /// Failed; platform's message.
    signal failed(string message)

    /// Start into fresh capture.
    function start() {
        if (recorder.recording || !recorder.available) {
            return
        }
        var path = captures.new_path("voice", recorder.extension)
        if (path.length > 0) {
            recorder.start(path)
        }
    }

    /// Stop; hand over when finished.
    function send() {
        recorder.stop()
    }

    /// Stop and discard.
    function cancel() {
        recorder.cancel()
    }

    /// m:ss from ms.
    function clock(milliseconds) {
        var total = Math.floor(milliseconds / 1000)
        var seconds = total % 60
        return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    Captures {
        id: captures
        objectName: "captures"
        onError: root.failed(message)
    }

    VoiceRecorder {
        id: recorder
        objectName: "recorder"
        limit_bytes: root.limitBytes
        media_quality: root.mediaQuality
        onRecorded: root.recorded(path)
        onError: root.failed(message)
    }

    // Polled, not signalled: cheap while recording, none otherwise.
    Timer {
        id: ticker
        objectName: "ticker"
        interval: 250
        repeat: true
        running: recorder.recording
        onTriggered: recorder.poll()
    }

    // Own flag, not `visible` (see ReplyBar).
    readonly property bool shown: recorder.recording
    visible: root.shown
    height: root.shown ? Math.max(timeLabel.height, cancelButton.height) : 0

    // Red dot: recording.
    Rectangle {
        id: dot
        objectName: "recordingDot"
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        width: Theme.paddingLarge
        height: width
        radius: width / 2
        color: Theme.errorColor

        // Blink: stopped strip reads stopped.
        SequentialAnimation on opacity {
            running: recorder.recording
            loops: Animation.Infinite
            NumberAnimation { to: 0.2; duration: 600 }
            NumberAnimation { to: 1.0; duration: 600 }
        }
    }

    Label {
        id: timeLabel
        objectName: "recordingTime"
        anchors {
            left: dot.right
            leftMargin: Theme.paddingMedium
            right: cancelButton.left
            rightMargin: Theme.paddingMedium
            verticalCenter: parent.verticalCenter
        }
        truncationMode: TruncationMode.Fade
        color: Theme.primaryColor
        //: Shown while a voice message records. %1 is the time so far, such as "0:07".
        text: qsTr("Recording %1").arg(root.clock(recorder.duration_ms))
    }

    IconButton {
        id: cancelButton
        objectName: "cancelRecordingButton"
        anchors {
            right: parent.right
            verticalCenter: parent.verticalCenter
        }
        icon.source: "image://theme/icon-m-clear"
        onClicked: root.cancel()
    }
}
