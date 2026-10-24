import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * What to start: chat, group, channel, each with a line on what it is; from chat list's
 * "New". Built as AddProfilePage.qml. Does nothing itself: picked page takes this one's
 * place, so a chat opened there sits on chat list, and back skips this choice.
 */
Page {
    id: page

    property int accountId

    function start(name) {
        // Literal paths: tree test follows them.
        var target = name === "newChat" ? Qt.resolvedUrl("NewChatPage.qml")
                                        : Qt.resolvedUrl("NewGroupPage.qml")
        var properties = { accountId: page.accountId }
        if (name === "newChannel") {
            properties.channel = true
        }
        if (pageStack.animatorReplace) {
            pageStack.animatorReplace(target, properties)
        } else {
            pageStack.replace(target, properties)
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        // Holds tiles; never shorter than page.
        contentHeight: Math.max(height, ways.y + ways.height + Theme.paddingLarge)

        PageHeader {
            id: header
            objectName: "header"
            //: Heading of the page that picks a new chat, group or channel.
            title: qsTr("New")
        }

        // Stacked: each tile has a line under its name.
        ChoiceTiles {
            id: ways
            objectName: "newWays"
            stacked: true
            width: page.width
            y: header.height + Theme.paddingLarge
            choices: [
                {
                    name: "newChat",
                    icon: "icon-m-chat",
                    text: qsTr("New chat"),
                    hint: qsTr("You write to one person.")
                },
                {
                    name: "newGroup",
                    icon: "icon-m-users",
                    text: qsTr("New group"),
                    hint: qsTr("Several people write to each other.")
                },
                {
                    name: "newChannel",
                    icon: "icon-m-speaker",
                    //: Delta Chat's "new_channel".
                    text: qsTr("New channel"),
                    //: Delta Chat's "chat_new_channel_hint".
                    hint: qsTr("Channels are a one-to-many tool for broadcasting your messages.")
                }
            ]
            onChosen: page.start(name)
        }
    }
}
