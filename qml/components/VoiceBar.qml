import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0

/*
 * A voice message being recorded, where the message field was.
 *
 * Its own component for the reason ReplyBar and AttachmentBar are:
 * ConversationPage cannot be loaded headlessly, so anything whose
 * behaviour matters has to be testable on its own. The recorder is here
 * too, so the page owns nothing but the two answers -- a recording to
 * send, or nothing.
 *
 * Tap to start, tap to send: a hold-to-record gesture is the other
 * clients' choice, and a tap is the one that cannot be let go of by
 * accident and that a reader can put the phone down during. While
 * recording, the strip is a red dot, the time, a cross to throw the
 * recording away, and the send button where it always is. The page
 * hides its field and attach button meanwhile.
 */
Item {
    id: root

    /// Whether anything can be recorded on this device.
    readonly property bool available: recorder.available
    /// A recording is running, or being finished.
    readonly property bool recording: recorder.recording
    /// The recording is finished and at `path`: send it.
    signal recorded(string path)
    /// Recording failed. The message is the platform's own.
    signal failed(string message)

    /// Start recording, into a fresh capture.
    function start() {
        if (recorder.recording || !recorder.available) {
            return
        }
        var path = captures.new_path("voice", recorder.extension)
        if (path.length > 0) {
            recorder.start(path)
        }
    }

    /// Stop, and hand the recording over once it is finished.
    function send() {
        recorder.stop()
    }

    /// Stop, and throw the recording away.
    function cancel() {
        recorder.cancel()
    }

    /// m:ss from milliseconds, which is what the recorder reports.
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
        onRecorded: root.recorded(path)
        onError: root.failed(message)
    }

    // The recorder is polled rather than connected to: one call a few
    // times a second while recording, and nothing at all otherwise.
    Timer {
        id: ticker
        objectName: "ticker"
        interval: 250
        repeat: true
        running: recorder.recording
        onTriggered: recorder.poll()
    }

    // Sized by its own reason to be here rather than by `visible`: see
    // ReplyBar.
    readonly property bool shown: recorder.recording
    visible: root.shown
    height: root.shown ? Math.max(timeLabel.height, cancelButton.height) : 0

    // The red dot: recording is under way.
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

        // Blinks, so a strip that has stopped moving reads as stopped.
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
