import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Which kind of delete, asked before anything goes.
 *
 * Deleting a message takes it off this account's devices and leaves
 * everybody else's copy where it is; the core will also ask the other
 * ends to delete theirs (`delete_messages_for_all`). Two different
 * things behind one word, and the difference -- who keeps a copy -- is
 * not one a thumb reads off a long-press menu. So the row menu keeps its
 * single Delete and this is where it leads.
 *
 * A Dialog rather than a page of answers: the reader picks one of the
 * two and swipes forward, which is how everything else on the phone is
 * accepted, and the back edge is the way out. Nothing is picked when it
 * opens and the forward swipe is not there until something is
 * (`canAccept`) -- the two are not each other's default, and a dialog
 * that opened with one already set would delete the wrong way for
 * somebody who swiped without reading.
 *
 * Nothing is deleted here. Whoever pushed this connects to `picked`, and
 * what answers it is the wait before the message goes
 * (components/PendingRemoval.qml) -- so a tap on the countdown is still
 * the last word. A dialog left by the back edge deletes nothing at all.
 */
Dialog {
    id: dialog

    /// Whether the core would take a deletion for everyone: a message
    /// this account sent, and an encrypted one. The row that asked knows
    /// both (`is_outgoing`, `show_padlock`); the core refuses the rest,
    /// and a switch that cannot work is better greyed than answered with
    /// an error afterwards.
    property bool canDeleteForEveryone: false

    /// Which way the reader has picked: "me", "everyone", or nothing
    /// yet, which is what it opens as.
    property string choice: ""

    /// The reader swiped forward on one of the two.
    signal picked(bool forEveryone)

    canAccept: dialog.choice.length > 0
    onAccepted: dialog.picked(dialog.choice === "everyone")

    /// Turn one on, and the other off with it. A second tap on the one
    /// already on turns it off, which puts the dialog back to the way it
    /// opened -- with nothing to accept.
    function choose(which) {
        dialog.choice = dialog.choice === which ? "" : which
    }

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        DialogHeader {
            objectName: "deleteHeader"
            //: Heading of the dialog asking which kind of delete.
            title: qsTr("Delete message?")
            acceptText: qsTr("Delete")
            cancelText: qsTr("Cancel")
        }

        // What the question is, under the heading that asks it: a
        // header's own line is drawn as whatever markup it is handed
        // (see ConversationHeader), and this one has something to say
        // that the two switches do not -- that there is no way back from
        // either of them.
        Label {
            objectName: "deleteQuestion"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.secondaryHighlightColor
            text: qsTr("How do you want to delete this message? This cannot be undone.")
        }

        // `automaticCheck: false` on both: Silica's own switch flips
        // itself on a tap, and these two are one answer rather than two
        // settings -- turning one on turns the other off, which is this
        // dialog's to decide rather than each switch's.
        TextSwitch {
            objectName: "forMeSwitch"
            //: One of the two ways to delete a message.
            text: qsTr("Delete for me")
            // Who keeps a copy is the whole difference between the two,
            // and it does not fit in a switch's own three words.
            description: qsTr("It goes from your devices. Everybody else keeps their copy.")
            automaticCheck: false
            checked: dialog.choice === "me"
            onClicked: dialog.choose("me")
        }

        TextSwitch {
            objectName: "forEveryoneSwitch"
            //: The other way: every other device in the chat is asked to
            //: delete its copy too.
            text: qsTr("Delete for everyone")
            description: qsTr("It goes from your devices, and every other device in this chat is asked to delete it too.")
            automaticCheck: false
            checked: dialog.choice === "everyone"
            enabled: dialog.canDeleteForEveryone
            onClicked: dialog.choose("everyone")
        }

        // Why the second one is greyed, said where it is rather than
        // after a failure: the core refuses both cases outright, and
        // being told afterwards is being told too late.
        Label {
            objectName: "deleteWhyNot"
            visible: !dialog.canDeleteForEveryone
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: qsTr("Only an encrypted message you sent yourself can be deleted for everyone.")
        }
    }
}
