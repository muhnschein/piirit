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
 *
 * The chats are one profile's, `accountId`. A caller whose chat can be any
 * profile's -- a quick action -- lets the reader turn the list to another
 * with `profileChoice`, and reads `accountId` back for whose chat was
 * picked. Forwarding and sharing do not: they send from the profile they
 * were started in.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    /// Whether the reader may turn the list to another profile's chats,
    /// with a choice of profile over it. Offered only while there is more
    /// than one profile to choose from.
    property bool profileChoice: false

    /// How many profiles the list could be turned to: the configured ones.
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

    /// The profile to turn the list to when `accountId` is none of the
    /// profiles there are -- a quick action's, whose chat was in a
    /// profile deleted since. 0 for the first there is.
    property int fallbackAccountId: 0

    /// A list opened on a profile that has gone is a list of nothing but
    /// "account not found", and with one profile left there is no choice
    /// over it to turn it by: it is turned to a profile there is. Only
    /// with a choice of profile; a picker without one sends from the
    /// profile it was started in, which is there.
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

    // Once the profiles are all in, however many there are: they arrive
    // one item at a time.
    Timer {
        id: settle
        interval: 0
        onTriggered: page.settleProfile()
    }

    /// What a profile goes by in the choice: its name, or its address
    /// without one.
    function profileName(accountId) {
        for (var i = 0; i < profiles.count; i++) {
            var item = profiles.itemAt(i)
            if (item && item.accountId === accountId) {
                return item.text
            }
        }
        return ""
    }

    // Another profile's chats are no answer yet either, and what the last
    // one's said is not about these.
    onAccountIdChanged: {
        page.chatsLoaded = false
        page.errorMessage = ""
    }

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

        // Whose chats these are, when there is a choice: every configured
        // profile, the way the profiles page names it.
        ComboBox {
            objectName: "pickerProfileCombo"
            width: parent.width
            visible: page.profileChoice && page.profileCount > 1
            //: Over a list of chats to pick from: which profile's chats
            //: they are.
            label: qsTr("Profile")
            // The profile the list is on, whichever item Silica last moved
            // the choice to.
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
