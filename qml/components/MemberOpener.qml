import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Tap on group member: their contact page if a chat with them exists, else the
 * row's menu (Send message). Never makes an empty chat by tapping. Send message:
 * chat made, opened as chat list opens chats.
 */
QtObject {
    id: root

    /// Group's ChatInfo: asks core.
    property QtObject chatInfo
    property int accountId

    property Item _row: null
    property int _asked: 0
    property string _name: ""
    property int _making: 0

    function open(row, contactId, name) {
        root._row = row
        root._asked = contactId
        root._name = name
        root.chatInfo.chat_with(contactId, false)
    }

    function sendMessage(contactId, name) {
        root._making = contactId
        root._name = name
        root.chatInfo.chat_with(contactId, true)
    }

    function _openChat(chatId, name) {
        // Via the list page: it tracks the open chat for notifications.
        if (typeof appWindow !== "undefined" && appWindow.chatList !== null
                && appWindow.chatList.accountId === root.accountId) {
            appWindow.chatList.openChatAbove(chatId, name)
            return
        }
        pageStack.push(Qt.resolvedUrl("../pages/ConversationPage.qml"), {
            accountId: root.accountId,
            chatId: chatId,
            chatName: name
        })
    }

    property Connections _answers: Connections {
        target: root.chatInfo
        onChat_found: {
            if (contact_id === root._making) {
                root._making = 0
                if (chat_id > 0) {
                    root._openChat(chat_id, root._name)
                }
            } else if (contact_id === root._asked) {
                root._asked = 0
                if (chat_id > 0) {
                    pageStack.push(Qt.resolvedUrl("../pages/ContactPage.qml"), {
                        accountId: root.accountId,
                        chatId: chat_id,
                        chatName: root._name
                    })
                } else if (root._row) {
                    root._row.openMenu()
                }
            }
        }
    }
}
