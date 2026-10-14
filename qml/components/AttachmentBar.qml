import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Format.js" as Format

/*
 * File the next message carries, above field.
 *
 * Own component like ReplyBar: ConversationPage can't load headless, so measurable parts must test alone. Looks like ReplyBar on purpose: same "next message has this" idiom.
 */
Item {
    id: root

    /// Picked file; empty for none. Makes bar appear.
    property string filePath
    /// Display name. Falls back to path when picker gave none.
    property string fileName
    /// Video being shrunk before send; progress 0 to 1. See
    /// ChatMessages.preparing.
    property bool preparing: false
    property real progress: 0
    /// Picked size while sending shrunk video; 0 otherwise (no sizes shown).
    /// See ChatMessages.original_bytes.
    property real originalBytes: 0
    /// Size sent; while still shrinking, planned size. See
    /// ChatMessages.attachment_bytes.
    property real bytes: 0
    /// File removed.
    signal cancelled()
    /// Shrinking stopped; file kept, sent as is. Same button, other meaning
    /// while shrinking.
    signal stopped()

    readonly property string shownName:
        root.fileName.length > 0 ? root.fileName : root.filePath

    // Sized by `shown`, not `visible`: see ReplyBar.
    readonly property bool shown: root.filePath.length > 0
    visible: root.shown
    // Both heights: cancel button is icon-tall. One-line quote measured short
    // in ReplyBar, bar overlapped field.
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
            // Name cut, not grown. No name while shrinking: wraps, since number
            // at end matters in long languages.
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

        // How much smaller the video goes: what was picked, what is sent.
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

    // Shrink progress: line along bottom of bar, fills as it goes.
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
