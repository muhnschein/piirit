import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Which delete, asked before anything goes. Delete: off this account's
 * devices, others keep copies. "For everyone" also asks other ends to delete
 * theirs (`delete_messages_for_all`). Row menu keeps one Delete; this asks.
 *
 * Dialog, not page: nothing preset, so a blind swipe cannot delete the wrong
 * way. Back edge deletes nothing. Caller connects to `picked`; the wait in
 * components/PendingRemoval.qml answers it, tap on countdown is last word.
 */
Dialog {
    id: dialog

    /// Core allows delete-for-everyone: own, encrypted messages only (row's
    /// `is_outgoing`, `show_padlock`).
    /// Greyed, not error, when not.
    property bool canDeleteForEveryone: false

    /// "me", "everyone", or "" until picked.
    property string choice: ""

    /// Reader swiped forward on one.
    signal picked(bool forEveryone)

    canAccept: dialog.choice.length > 0
    onAccepted: dialog.picked(dialog.choice === "everyone")

    /// Turn one on, other off. Tap again clears.
    function choose(which) {
        dialog.choice = dialog.choice === which ? "" : which
    }

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        // No title: the question says it. Accept and cancel come from header.
        DialogHeader {
            objectName: "deleteHeader"
            acceptText: qsTr("Delete")
            cancelText: qsTr("Cancel")
        }

        // Question, incl. the no-undo part neither switch says.
        Label {
            objectName: "deleteQuestion"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.secondaryHighlightColor
            text: qsTr("How do you want to delete this message? This cannot be undone.")
        }

        // `automaticCheck: false`: Silica switch flips itself on tap; these are
        // one answer, so dialog decides (one on turns other off).
        TextSwitch {
            objectName: "forMeSwitch"
            //: One of the two ways to delete a message.
            text: qsTr("Delete for me")
            automaticCheck: false
            checked: dialog.choice === "me"
            onClicked: dialog.choose("me")
        }

        TextSwitch {
            objectName: "forEveryoneSwitch"
            //: The other way: every other device in the chat is asked to
            //: delete its copy too.
            text: qsTr("Delete for everyone")
            automaticCheck: false
            checked: dialog.choice === "everyone"
            enabled: dialog.canDeleteForEveryone
            onClicked: dialog.choose("everyone")
        }

        // Why greyed, said up front: telling after a failure is too late.
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
