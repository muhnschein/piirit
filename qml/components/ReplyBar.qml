import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Emoji.js" as Emoji

/*
 * Reply/edit bar above field; own file so wrap is testable (ConversationPage
 * won't load headless).
 */
Item {
    id: root

    property string author
    property string body
    /// Editing sent message: bar says so; cancel restores field.
    property bool editing: false
    // Enough to recognise, not crowd screen. Rest ellipsised.
    property int maximumLines: 3
    signal cancelled()

    // Own flag: `visible` goes false under another page (MessageDelegate).
    readonly property bool shown: root.body.length > 0 || root.author.length > 0
    // One joined `arg`: 2nd call rescans output (contact "%2" misplaced).
    readonly property string quotedText: root.editing
                                         //: Above the message field while a sent message's text is
                                         //: being changed; the message's text follows.
                                         ? qsTr("Editing message") + ": " + root.body
                                         : qsTr("Replying to %1").arg(root.author) + ": " + root.body
    /// Same with emoji pictures, muted as chat list's; "" for as written.
    readonly property string emojiQuoted: Emoji.inText(root.quotedText, false,
                                                       Theme.fontSizeExtraSmall,
                                                       Emoji.previewSaturation)
    visible: root.shown
    // Measure cancel too: its icon is taller than a one-line quote.
    height: root.shown ? Math.max(quoted.height, cancel.height) + 2 * Theme.paddingSmall : 0

    Label {
        id: quoted
        objectName: "replyLabel"
        anchors.verticalCenter: parent.verticalCenter
        x: Theme.horizontalPageMargin
        width: parent.width - x - cancel.width - Theme.paddingMedium
        wrapMode: Text.Wrap
        maximumLineCount: root.maximumLines
        // Cut, not elided, with pictures: elided last line misplaces them (MessageDelegate body).
        truncationMode: root.emojiQuoted.length > 0 ? TruncationMode.None : TruncationMode.Elide
        elide: root.emojiQuoted.length > 0 ? Text.ElideNone : Text.ElideRight
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: root.emojiQuoted.length > 0 ? Text.StyledText : Text.PlainText
        text: root.emojiQuoted.length > 0 ? root.emojiQuoted : root.quotedText
    }

    IconButton {
        id: cancel
        objectName: "cancelReplyButton"
        anchors {
            verticalCenter: parent.verticalCenter
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        icon.source: "image://theme/icon-m-clear"
        onClicked: root.cancelled()
    }
}
