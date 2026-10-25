import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * What to start: chat, group, channel, each with a line on what it is; from chat list's
 * "New". Built as DeleteMessageDialog.qml: one switch at a time, nothing preset, forward
 * swipe only once picked. Picked page goes on top, so back from it returns here; chat made
 * there opens over `landOn` (chat list), this dialog gone with it.
 */
Dialog {
    id: dialog

    property int accountId
    /// Page new chat opens over; opener passes itself.
    property var landOn: null

    /// "chat", "group", "channel", or "" until picked.
    property string choice: ""

    canAccept: dialog.choice.length > 0
    // Literal paths: tree test follows them.
    acceptDestination: dialog.choice === "chat" ? Qt.resolvedUrl("NewChatPage.qml")
                     : dialog.choice.length > 0 ? Qt.resolvedUrl("NewGroupPage.qml")
                     : ""
    // Chat page has no `channel`: only group page gets it.
    acceptDestinationProperties: dialog.choice === "chat"
                                 ? { accountId: dialog.accountId, landOn: dialog.landOn }
                                 : { accountId: dialog.accountId, landOn: dialog.landOn,
                                     channel: dialog.choice === "channel" }

    // Silica builds destination once, when dialog opens or URL changes: group to channel
    // keeps same page, built with old `channel`. Current pick set on it here.
    onAccepted: {
        if (dialog.choice !== "chat") {
            dialog.acceptDestinationInstance.channel = dialog.choice === "channel"
        }
    }

    /// Turn one on, others off. Tap again clears.
    function choose(which) {
        dialog.choice = dialog.choice === which ? "" : which
    }

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        // No title: pull-down's "New" said it; switches say rest.
        DialogHeader {
            objectName: "header"
            //: Goes on to the page for the picked chat, group or channel.
            acceptText: qsTr("Next")
        }

        // `automaticCheck: false`: Silica switch self-flips; one answer, dialog decides.
        TextSwitch {
            objectName: "chatSwitch"
            text: qsTr("New chat")
            description: qsTr("You write to one person.")
            automaticCheck: false
            checked: dialog.choice === "chat"
            onClicked: dialog.choose("chat")
        }

        TextSwitch {
            objectName: "groupSwitch"
            text: qsTr("New group")
            description: qsTr("Several people write to each other.")
            automaticCheck: false
            checked: dialog.choice === "group"
            onClicked: dialog.choose("group")
        }

        TextSwitch {
            objectName: "channelSwitch"
            //: Delta Chat's "new_channel".
            text: qsTr("New channel")
            //: Delta Chat's "chat_new_channel_hint".
            description: qsTr("Channels are a one-to-many tool for broadcasting your messages.")
            automaticCheck: false
            checked: dialog.choice === "channel"
            onClicked: dialog.choose("channel")
        }
    }
}
