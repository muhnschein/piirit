import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Emoji.js" as Emoji

/*
 * Contact request's answer, in field's place (nothing to send till accepted): why,
 * then turn-down and Accept side by side, as reference clients. One-to-one turns
 * down with Block; group with Delete (whole group not blocked this way). Says,
 * never acts: page answers signals.
 */
Item {
    id: root

    /// Request unanswered: bar up. Own flag: `visible` goes false under another page.
    property bool shown: false
    /// Group request: Delete, not Block.
    property bool isGroup: false
    /// Who wrote, for one-to-one text.
    property string chatName
    /// Why the bar is up, as written.
    readonly property string sentence: root.isGroup
        //: Over the buttons of a group the account was added to by
        //: someone it has never written to.
        ? qsTr("Someone who isn't in your contacts added you to this group. Accept to reply, or delete the group.")
        //: Over the buttons of a chat started by someone the
        //: account has never written to. %1 is their name. Core
        //: sends no read receipts for these messages, ever.
        : qsTr("%1 isn't in your contacts yet. Accept to reply. They won't be told that you've read their messages.")
          .arg(root.chatName)
    /// Same, name's emoji as Twemoji; "" for plain.
    readonly property string pictured: Emoji.inText(root.sentence, false, Theme.fontSizeSmall,
                                                    Emoji.nameTint(Theme.highlightColor))
    signal acceptClicked()
    signal declineClicked()

    visible: root.shown
    height: root.shown ? column.height + 2 * Theme.paddingLarge : 0

    // Tinted: set apart from messages above (list ends at bar).
    Rectangle {
        anchors.fill: parent
        color: Theme.rgba(Theme.highlightBackgroundColor, Theme.highlightBackgroundOpacity)
    }

    Column {
        id: column
        x: Theme.horizontalPageMargin
        y: Theme.paddingLarge
        width: parent.width - 2 * Theme.horizontalPageMargin
        spacing: Theme.paddingLarge

        Label {
            id: explanation
            objectName: "requestText"
            width: parent.width
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.highlightColor
            // Peer-chosen name: plain text, or escaped StyledText with pictures.
            textFormat: root.pictured.length > 0 ? Text.StyledText : Text.PlainText
            text: root.pictured.length > 0 ? root.pictured : root.sentence
        }

        Row {
            width: parent.width
            spacing: Theme.paddingLarge

            Button {
                id: decline
                objectName: "declineButton"
                width: (parent.width - parent.spacing) / 2
                text: root.isGroup ? qsTr("Delete") : qsTr("Block")
                onClicked: root.declineClicked()
            }

            Button {
                id: accept
                objectName: "acceptButton"
                width: (parent.width - parent.spacing) / 2
                color: Theme.highlightColor
                //: Accepts a contact request: the chat becomes an ordinary
                //: one and the message field appears.
                text: qsTr("Accept")
                onClicked: root.acceptClicked()
            }
        }
    }
}
