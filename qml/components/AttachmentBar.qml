import QtQuick 2.0
import Sailfish.Silica 1.0

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
    /// The file is put away.
    signal cancelled()
    /// Making the video smaller is stopped, and nothing is sent; the file
    /// stays. What the same button means while that runs: putting the
    /// file away under a send that is using it would leave the send
    /// holding a file the bar no longer shows.
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
    height: root.shown ? Math.max(attached.height, cancel.height) + 2 * Theme.paddingSmall : 0

    Label {
        id: attached
        objectName: "pendingAttachmentLabel"
        anchors.verticalCenter: parent.verticalCenter
        x: Theme.horizontalPageMargin
        width: parent.width - x - cancel.width - Theme.paddingMedium
        truncationMode: TruncationMode.Fade
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: Text.PlainText
        text: root.preparing
              //: Shown above the message field while a picked video is
              //: made smaller, which happens before it is sent so that
              //: relays take it. %1 is the file name, %2 how much of it
              //: is done, as a number from 0 to 100.
              ? qsTr("Making %1 smaller before sending it: %2%")
                .arg(root.shownName).arg(Math.round(root.progress * 100))
              //: Shown above the message field once a file has been
              //: picked. %1 is the file name.
              : qsTr("Sending %1").arg(root.shownName)
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
