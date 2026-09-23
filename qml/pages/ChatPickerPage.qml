import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Pick a chat to send something into, or for a quick action on the cover
 * to open.
 *
 * The list comes from the core with DC_GCL_FOR_FORWARDING, so it leaves
 * out the chats a forward would be refused by rather than offering them
 * and failing afterwards.
 *
 * Reports its answer with a signal rather than acting itself: what happens
 * to the chosen chat is the caller's business, and a picker that forwarded
 * on its own could not be reused for anything else.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    /// Whether picking a chat closes this page.
    ///
    /// True for forwarding, which stays where it was and sends from
    /// there. False for a share, which puts the chosen chat in this
    /// page's place -- and a page that popped itself first would take
    /// the chat with it.
    property bool closeOnPick: true

    /// What the header says the chat is being picked for, and what the
    /// list says when there is nothing in it. Forwarding's words unless
    /// the page is opened for something else.
    property string title: qsTr("Forward to")
    property string emptyText: qsTr("No chats to forward to")

    /// The reader picked this chat.
    signal chatPicked(int chatId, string chatName)

    Timer {
        id: searchDebounce
        interval: 250
        onTriggered: chats.query = searchField.text
    }

    // The core has said which chats there are, whatever that was. An
    // empty list before then is no answer rather than no chats: see
    // ChatListPage.
    property bool chatsLoaded: false

    ChatList {
        id: chats
        objectName: "pickerChats"
        account_id: page.accountId
        for_forwarding: true
        onError: page.errorMessage = message
        // Emitted once the rows have been set, whether there turned out
        // to be any or none.
        onRows_changed: page.chatsLoaded = true
    }

    Connections {
        target: core
        onCore_event: chats.handle_event(context_id, kind, payload_json)
    }

    // Outside the list for the reason ChatListPage documents: a field in
    // a view's `header` lives inside the flickable, and its id does not
    // resolve from the page -- so the debounce below was reading an
    // undefined name and this search did nothing at all.
    Column {
        id: heading
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
        }

        PageHeader {
            title: page.title
        }

        SearchField {
            id: searchField
            objectName: "pickerSearchField"
            width: parent.width
            placeholderText: qsTr("Search")
            onTextChanged: searchDebounce.restart()
        }
    }

    SilicaListView {
        id: listView
        anchors {
            top: heading.bottom
            left: parent.left
            right: parent.right
            bottom: banner.top
        }
        clip: true
        model: chats.rows

        // No context menu: choosing is the only thing to do with a row.
        delegate: ListItem {
            objectName: "pickerRow"
            contentHeight: body.height

            ChatListDelegate {
                id: body
                width: parent.width
                chatName: model.name
                preview: model.preview
                previewSender: model.preview_sender
                lastUpdated: model.last_updated
                isEncrypted: model.is_encrypted
                isPinned: model.is_pinned
                isMuted: model.is_muted
                chatColor: model.color
                avatarPath: model.avatar_path
                // Not a chat being read: an unread badge here is noise.
                unreadCount: 0
            }

            onClicked: {
                page.chatPicked(model.chat_id, model.name)
                if (page.closeOnPick) {
                    pageStack.pop()
                }
            }
        }

        ViewPlaceholder {
            objectName: "chatsPlaceholder"
            // Not until the core has answered: see `chatsLoaded`.
            enabled: page.chatsLoaded && chats.count === 0
            text: page.emptyText
        }
    }

    Banner {
        id: banner
        objectName: "errorBanner"
        anchors.bottom: parent.bottom
        width: parent.width
        text: page.errorMessage
        onDismissed: page.errorMessage = ""
    }
}
