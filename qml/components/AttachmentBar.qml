import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Format.js" as Format

/*
 * The file the next message will carry, above the field.
 *
 * Its own component for the same reason ReplyBar is: ConversationPage
 * cannot be loaded headlessly, so anything whose measurement matters has to
 * be testable on its own. It also deliberately looks like ReplyBar -- both
 * say "the next message will have this in it", and one idiom for that is
 * easier to read than two.
 */
Item {
    id: root

    /// The picked file, empty for none. This is what makes the bar appear.
    property string filePath
    /// What to call it. Falls back to the path when a picker gave no name.
    property string fileName
    /// The file is a video being made smaller before it is sent, and
    /// how far along that is, from 0 to 1. See ChatMessages.preparing.
    property bool preparing: false
    property real progress: 0
    /// What the picked file weighs, while what is sent is a smaller video
    /// made from it; 0 otherwise, and then no sizes are shown. See
    /// ChatMessages.original_bytes.
    property real originalBytes: 0
    /// What is sent: while it is still being made, what it is planned to
    /// come out at. See ChatMessages.attachment_bytes.
    property real bytes: 0
    /// The file is put away.
    signal cancelled()
    /// Making the video smaller is stopped; the file stays, to be sent as
    /// it is. What the same button means while that runs.
    signal stopped()

    readonly property string shownName:
        root.fileName.length > 0 ? root.fileName : root.filePath

    // Sized by its own reason to be here rather than by `visible`: see
    // ReplyBar.
    readonly property bool shown: root.filePath.length > 0
    visible: root.shown
    // Both, not just the label: the cancel button is an icon's worth tall.
    // Same reasoning as ReplyBar, where a one-line quote measured short and
    // the bar overlapped the field below it.
    height: root.shown ? Math.max(lines.height, cancel.height) + 2 * Theme.paddingSmall : 0

    Column {
        id: lines
        anchors.verticalCenter: parent.verticalCenter
        x: Theme.horizontalPageMargin
        width: parent.width - x - cancel.width - Theme.paddingMedium

        Label {
            id: attached
            objectName: "pendingAttachmentLabel"
            width: parent.width
            // A file name is cut rather than let grow the bar. While the video
            // is made smaller there is no name, and the line wraps instead:
            // in a long language the number at its end is what matters.
            truncationMode: root.preparing ? TruncationMode.None : TruncationMode.Fade
            wrapMode: root.preparing ? Text.Wrap : Text.NoWrap
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            textFormat: Text.PlainText
            text: root.preparing
                  //: Shown above the message field while a picked video is
                  //: made smaller, which happens before it is sent so that
                  //: relays take it. %1 is how much of it is done, as a
                  //: number from 0 to 100. Keep it short: it shares a line
                  //: with a button on a phone.
                  ? qsTr("Making video smaller for sending: %1%")
                    .arg(Math.round(root.progress * 100))
                  //: Shown above the message field once a file has been
                  //: picked. %1 is the file name.
                  : qsTr("Sending %1").arg(root.shownName)
        }

        // How much smaller the video goes: what was picked, and what is sent.
        Label {
            objectName: "attachmentSizesLabel"
            visible: root.originalBytes > 0
            width: parent.width
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            textFormat: Text.PlainText
            text: !visible
                  ? ""
                  : root.preparing
                  //: Under the line saying a picked video is being made
                  //: smaller: %1 is what the video weighs as picked, %2 what
                  //: it is planned to come out at, each such as "24 MB". The
                  //: second is a guess until it is made, hence the tilde.
                  ? qsTr("%1 → ~%2").arg(Format.readableSize(root.originalBytes))
                    .arg(Format.readableSize(root.bytes))
                  //: Under the line saying a picked video is sent, once it
                  //: has been made smaller: %1 is what the video weighed as
                  //: picked, %2 what is sent, each such as "24 MB".
                  : qsTr("%1 → %2").arg(Format.readableSize(root.originalBytes))
                    .arg(Format.readableSize(root.bytes))
        }
    }

    // How far along making the video smaller is: a line along the bottom
    // of the bar, filling as it goes.
    Rectangle {
        objectName: "preparingProgress"
        anchors {
            left: parent.left
            bottom: parent.bottom
        }
        visible: root.preparing
        height: Math.max(2, Theme.paddingSmall / 2)
        width: parent.width * Math.max(0, Math.min(1, root.progress))
        color: Theme.highlightColor
    }

    IconButton {
        id: cancel
        objectName: "cancelAttachmentButton"
        anchors {
            verticalCenter: parent.verticalCenter
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        icon.source: "image://theme/icon-m-clear"
        onClicked: root.preparing ? root.stopped() : root.cancelled()
    }
}
