import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Pick a chat to send into, or for a quick action to open.
 * Core list (DC_GCL_FOR_FORWARDING): no chats a forward would refuse.
 * Answers by signal; caller acts.
 *
 * One profile's chats (`accountId`). Quick action: `profileChoice` switches
 * profile. Forward and share send from start profile.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    /// Offer other profiles' chats. Only with 2+ profiles.
    property bool profileChoice: false

    /// Profiles to turn to: configured ones.
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

    /// Turn-to profile when accountId is gone (deleted quick-action profile). 0
    /// for first.
    property int fallbackAccountId: 0

    /// Gone profile lists only "account not found": turn to an existing one.
    /// Only with `profileChoice`; without, picker sends from start profile,
    /// which exists.
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

    // Other profile's chats: no answer yet, and last profile's error is stale.
    onAccountIdChanged: {
        page.chatsLoaded = false
        page.errorMessage = ""
    }

    /// Closes page when picked? True for forward (sends from here). False for
    /// share: chat takes this page's place; popping first would drop it.
    property bool closeOnPick: true

    /// Header and empty-list words: forward's by default.
    property string title: qsTr("Forward to")
    property string emptyText: qsTr("No chats to forward to")

    /// Reader picked this chat.
    signal chatPicked(int chatId, string chatName)

    Timer {
        id: searchDebounce
        interval: 250
        onTriggered: chats.query = searchField.text
    }

    // Core answered, however empty. Empty before is no answer: see
    // ChatListPage.
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

    // Outside list (see ChatListPage): header field's id won't resolve from
    // page, so debounce read undefined and search did nothing.
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

        // Whose chats, when a choice exists: profile names as on profiles page.
        ComboBox {
            objectName: "pickerProfileCombo"
            width: parent.width
            visible: page.profileChoice && page.profileCount > 1
            //: Over a list of chats to pick from: which profile's chats
            //: they are.
            label: qsTr("Profile")
            // Profile list is on, whatever item Silica last moved choice to.
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

        // No context menu: choosing is the only row action.
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
                // Unread badge is noise: not a chat being read.
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
