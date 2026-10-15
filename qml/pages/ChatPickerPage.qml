import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Pick a chat to send into, or for a quick action. Core list
 * DC_GCL_FOR_FORWARDING (no refusable chats); answers by signal.
 *
 * One profile's chats (`accountId`); quick action may switch (`profileChoice`),
 * forward and share stay on start profile.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    /// Offer other profiles (2+ only).
    property bool profileChoice: false

    /// Configured profiles.
    readonly property int profileCount: {
        var count = 0
        for (var i = 0; i < profiles.count; i++) {
            var item = profiles.itemAt(i)
            if (item && item.configured) {
                count++
            }
        }
        return count
    }

    /// Fallback when accountId gone (deleted quick-action profile); 0: first.
    property int fallbackAccountId: 0

    /// Gone profile lists only "account not found": switch. Only with
    /// `profileChoice` (else start profile, which exists).
    function settleProfile() {
        if (!page.profileChoice || profiles.count === 0
                || page.profileName(page.accountId) !== "") {
            return
        }
        var first = 0
        for (var i = 0; i < profiles.count; i++) {
            var item = profiles.itemAt(i)
            if (!item || !item.configured) {
                continue
            }
            if (item.accountId === page.fallbackAccountId) {
                page.accountId = item.accountId
                return
            }
            if (first === 0) {
                first = item.accountId
            }
        }
        if (first > 0) {
            page.accountId = first
        }
    }

    // Profiles arrive one at a time: wait for all.
    Timer {
        id: settle
        interval: 0
        onTriggered: page.settleProfile()
    }

    /// Profile label: name, else address.
    function profileName(accountId) {
        for (var i = 0; i < profiles.count; i++) {
            var item = profiles.itemAt(i)
            if (item && item.accountId === accountId) {
                return item.text
            }
        }
        return ""
    }

    // Other profile: no answer yet, old error stale.
    onAccountIdChanged: {
        page.chatsLoaded = false
        page.errorMessage = ""
    }

    /// Pop on pick: forward yes; share no (chat replaces page, pop would drop it).
    property bool closeOnPick: true

    /// Header and empty text; forward's default.
    property string title: qsTr("Forward to")
    property string emptyText: qsTr("No chats to forward to")

    /// Reader picked this chat.
    signal chatPicked(int chatId, string chatName)

    Timer {
        id: searchDebounce
        interval: 250
        onTriggered: chats.query = searchField.text
    }

    // Core answered, even empty (see ChatListPage).
    property bool chatsLoaded: false

    ChatList {
        id: chats
        objectName: "pickerChats"
        account_id: page.accountId
        for_forwarding: true
        onError: page.errorMessage = message
        onRows_changed: page.chatsLoaded = true
    }

    Connections {
        target: core
        onCore_event: chats.handle_event(context_id, kind, payload_json)
    }

    // Outside list (see ChatListPage): header field id unresolvable from page.
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

        // Whose chats, when choice: names as profiles page.
        ComboBox {
            objectName: "pickerProfileCombo"
            width: parent.width
            visible: page.profileChoice && page.profileCount > 1
            //: Over a list of chats to pick from: which profile's chats
            //: they are.
            label: qsTr("Profile")
            // Shown profile, whatever Silica last moved choice to.
            value: page.profileName(page.accountId)

            menu: ContextMenu {
                Repeater {
                    id: profiles
                    model: core.account_list
                    onItemAdded: settle.restart()

                    MenuItem {
                        objectName: "pickerProfile" + model.account_id
                        readonly property int accountId: model.account_id
                        readonly property bool configured: model.is_configured
                        visible: configured
                        text: model.display_name.length > 0 ? model.display_name
                                                            : model.addr
                        onClicked: page.accountId = accountId
                    }
                }
            }
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

        // No context menu: picking is only action.
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
                // No unread badge: noise here.
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
            // Not until core answered: see chatsLoaded.
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
