import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Which kind of delete, asked on a page of its own.
 *
 * Deleting a message takes it off this account's devices and leaves
 * everybody else's copy where it is; the core will also ask the other
 * ends to delete theirs (`delete_messages_for_all`). Two different things
 * behind one word, and the difference is not one a thumb reads off a
 * long-press menu -- so the menu keeps its single Delete, and this is
 * where it leads. The message is on the screen above the two ways, so
 * that what is about to go is what the reader meant.
 *
 * Not a countdown and not a Dialog: a Dialog has one accept, and there
 * are two answers here that are not each other's opposite. ChoiceTiles
 * is what this app asks a question with when the answers are doors
 * (components/ChoiceTiles.qml), and the two tiles carry a line each
 * about who keeps a copy.
 *
 * Nothing is deleted here. The page that pushed this connects to
 * `picked`, and what answers it is the wait before the message goes
 * (components/PendingRemoval.qml) -- so the last word is still a tap on
 * the countdown, as it was. Leaving without picking, by the back gesture
 * or by Cancel, deletes nothing at all.
 */
Page {
    id: page

    /// Who wrote the message, as the conversation names them.
    property string senderName: ""
    /// What it says, empty for a message that is only an attachment.
    property string body: ""
    /// What it carries, for a message with no words in it.
    property string fileName: ""
    /// Whether the core would take a deletion for everyone: a message
    /// this account sent, and an encrypted one. The row that asked knows
    /// both (`is_outgoing`, `show_padlock`); the core refuses the rest,
    /// and a tile that cannot work is better greyed than answered with
    /// an error afterwards.
    property bool canDeleteForEveryone: false

    /// The reader picked one of the two.
    signal picked(bool forEveryone)

    /// The message as a line: its words, or what it carries when it has
    /// none.
    readonly property string preview: page.body.length > 0 ? page.body
                                                           : page.fileName

    /// Say which way, and go back to the conversation with it.
    function answer(forEveryone) {
        page.picked(forEveryone)
        pageStack.pop()
    }

    allowedOrientations: Orientation.All

    SilicaFlickable {
        objectName: "deleteFlickable"
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        VerticalScrollDecorator {}

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                objectName: "deleteHeader"
                //: Heading of the page asking which kind of delete.
                title: qsTr("Delete message")
            }

            // Who wrote it. Its own label rather than the header's
            // `description`, which draws whatever it is given as markup:
            // see ConversationHeader.
            Label {
                objectName: "deleteSender"
                visible: page.senderName.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                // The name is whatever they called themselves.
                textFormat: Text.PlainText
                text: page.senderName
            }

            // The message itself, as far as a few lines go: enough to
            // recognise it by, and never so much that the two ways below
            // are off the screen when the page opens.
            Label {
                objectName: "deletePreview"
                visible: page.preview.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                maximumLineCount: 6
                elide: Text.ElideRight
                color: Theme.highlightColor
                // A message, and a file name the other end chose with it.
                textFormat: Text.PlainText
                text: page.preview
            }

            Label {
                objectName: "deleteEmpty"
                visible: page.preview.length === 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
                //: Stands in for the message on the delete page, when it
                //: has no words and nothing named to show instead.
                text: qsTr("This message has no text")
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }

            // The two ways, one under the other rather than side by
            // side: each carries a line about who keeps a copy, and that
            // does not fit in half a screen (AddProfilePage stacks its
            // three for the same reason).
            //
            // The icons are two this app has already asked a device for
            // (ChoiceTiles says why that matters): the phone in your
            // hand, and something on its way to the other ends.
            ChoiceTiles {
                id: ways
                objectName: "deleteWays"
                stacked: true
                width: page.width
                choices: [
                    {
                        name: "forMe",
                        icon: "icon-m-device",
                        //: One of the two ways to delete a message.
                        text: qsTr("Delete for me"),
                        hint: qsTr("It goes from your devices. Everybody else keeps their copy.")
                    },
                    {
                        name: "forEveryone",
                        icon: "icon-m-transfer",
                        //: The other way: every other device in the chat
                        //: is asked to delete its copy too.
                        text: qsTr("Delete for everyone"),
                        hint: qsTr("It goes from your devices, and every other device in this chat is asked to delete it too."),
                        enabled: page.canDeleteForEveryone
                    }
                ]
                onChosen: page.answer(name === "forEveryone")
            }

            // Why the second way is greyed, said where the tile is
            // rather than after a failure: the core refuses both cases
            // outright, and being told afterwards is being told too late.
            Label {
                objectName: "deleteWhyNot"
                visible: !page.canDeleteForEveryone
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: qsTr("Only an encrypted message you sent yourself can be deleted for everyone.")
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }

            // A way out that is a tap rather than a gesture. The back
            // gesture does the same thing, and a page offering two
            // deletions should not be one a reader has to know a gesture
            // to leave.
            Button {
                objectName: "deleteCancel"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Cancel")
                onClicked: pageStack.pop()
            }
        }
    }
}
