import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

Page {
    id: page

    property int accountId
    /// Shows the archived chats instead of the ordinary ones. The two
    /// lists are disjoint, so this is a mode rather than a filter.
    property bool archived: false
    /// How many configured accounts there are, from the core. Decides
    /// whether switching is worth offering at all.
    property int accountCount: 0

    // Not bound straight to the field: a round trip per keystroke asks the
    // core four times to type "anna", and only the last answer is wanted.
    // Same interval as the contact search, for the same reason.
    Timer {
        id: searchDebounce
        interval: 250
        onTriggered: {
            // The archived list is a mode over one kind of thing, so it
            // filters itself. The ordinary list searches everything and
            // shows the three kinds grouped, the way the reference
            // clients do.
            if (page.archived) {
                chats.query = searchField.text
            } else {
                searchModel.query = searchField.text
            }
        }
    }

    /// Results are showing instead of the chat list. Read from the field
    /// rather than from the model, so the swap happens on the keystroke
    /// and not a debounce later.
    readonly property bool searching:
        !page.archived && searchField.text.trim().length > 0

    // Loads a chat before the page that shows it exists, so the
    // transition starts with the messages already in hand instead of
    // arriving empty and filling in behind itself.
    ChatPrefetch {
        id: prefetch
        objectName: "prefetch"
        account_id: page.accountId
        onReady: page.openLoadedChat(chat_id)
    }

    /// Where a tap is going, held while the prefetch runs: the answer
    /// carries only an id, and the name and the found message would
    /// otherwise be lost between the tap and the push.
    property string pendingChatName: ""
    property int pendingMessageId: 0

    function openChat(chatId, chatName, messageId) {
        page.pendingChatName = chatName
        page.pendingMessageId = messageId
        // Told before the push, so a message arriving during the
        // transition is not announced into the reader's face.
        notifier.viewingChatId = chatId
        // The message a search found goes in too: the page it opens on is
        // the one that message is in, rather than the newest page and a
        // jump away from it a moment later.
        prefetch.start(chatId, messageId ? messageId : 0)
    }

    function openLoadedChat(chatId) {
        // Only while this is the page on screen. The pulley menu stays
        // open while a chat loads, and a reader who used it to go to
        // Profiles in the meantime did not ask for a conversation on top.
        if (page.status !== PageStatus.Active) {
            return
        }
        pageStack.push(Qt.resolvedUrl("ConversationPage.qml"), {
            accountId: page.accountId,
            chatId: chatId,
            chatName: page.pendingChatName,
            findMessageId: page.pendingMessageId
        })
    }

    SearchResults {
        id: searchModel
        objectName: "search"
        account_id: page.accountId
        onError: page.errorMessage = message
        // A contact result has no chat until it is tapped.
        onChat_ready: page.openChat(chat_id, page.pendingChatName, 0)
    }

    ChatList {
        id: chats
        objectName: "chats"
        account_id: page.accountId
        archived: page.archived
        // A reply to one of the reader's own messages gets through a
        // muted group, when they want it to. `=== true` because dconf
        // hands back `undefined` before it has read the key.
        notify_mentions: Settings.mentionNotifications === true
        onError: page.errorMessage = message
    }

    // Sits here rather than in the root window because this is where the
    // chat list already lives: the model decides what counts as an
    // arrival, and the page knows which chat the reader walked into.
    Notifier {
        id: notifier
        objectName: "notifier"
        // Off, nothing is announced and what was is taken down; the
        // model still says what arrived, for the list's own sake.
        enabled: Settings.notificationsEnabled === true
        detail: Settings.notificationDetail
        // A tap on a notification: back to the list, then into the chat,
        // in front of whatever the reader was doing.
        onOpenRequested: page.showChat(chatId)
    }

    /// The core has said what is in this profile, whatever that was.
    ///
    /// An empty model means one of two things and they are opposite: no
    /// chats, or no answer yet. On a phone that resumes onto this page
    /// the second is what is true for the first moment, and saying "No
    /// chats yet" over a list that is about to fill is the app telling
    /// the reader something it does not know.
    property bool chatsLoaded: false

    Connections {
        target: chats
        onMessage_arrived: notifier.arrived(chat_id, chat_name, sender, preview)
        // Emitted once the rows have been set, whether there turned out
        // to be any or none.
        onRows_changed: page.chatsLoaded = true
    }

    /// Open a chat from outside the app: a notification was tapped. The
    /// window is raised first, as lipstick only calls the app and does
    /// not bring it up; a page loaded on its own in a test has no window.
    function showChat(chatId) {
        if (typeof appWindow !== "undefined") {
            appWindow.activate()
        }
        if (pageStack.currentPage !== page) {
            pageStack.pop(page, PageStackAction.Immediate)
        }
        page.openChat(chatId, notifier.nameOf(chatId), 0)
    }

    // Back on the list means no chat is being read. Going the other way,
    // any chat still waiting to be deleted goes now: leaving is exactly
    // when a timer has not fired yet, and somebody who asked for a chat
    // to go and then opened another one asked for it to go.
    onStatusChanged: {
        if (status === PageStatus.Active) {
            notifier.viewingChatId = 0
        } else if (status === PageStatus.Deactivating) {
            doomedChats.flush()
        }
    }

    /// How long a chat waits before it goes, in milliseconds. Nothing
    /// sets it; a test turns it down rather than waiting.
    property alias pendingDelay: doomedChats.delay

    /// The chats the reader has asked to delete, waiting out the moment
    /// in which they can say they did not mean it.
    ///
    /// Not Silica's `remorseAction`, which would put the wait on the row:
    /// a message arriving moves a chat up this list, which is a remove
    /// and an insert, so the row -- and the wait on it -- can go at any
    /// moment and for a reason that has nothing to do with the reader.
    /// See PendingRemoval.
    PendingRemoval {
        id: doomedChats
        onRemove: chats.delete_chat(id)
    }

    property string errorMessage: ""
    // Three states, not two: the core going away is now something the app
    // does something about, and a banner that says "restart Piirit"
    // while Piirit is already fixing it is worse than none.
    readonly property string coreStatusMessage:
        core.status === "reconnecting"
        ? qsTr("Lost the connection to the Delta Chat core. Reconnecting...")
        : core.status === "stopped"
          ? qsTr("Lost the connection to the Delta Chat core. Restart Piirit.")
          : ""

    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml. The model ignores
        // events for other accounts itself.
        onCore_event: chats.handle_event(context_id, kind, payload_json)
        onStatus_changed: {
            if (core.status === "ready") {
                // On a phone that resumes onto its chat list, this page
                // is the first one made and the core was not up when it
                // was: what it asked for then reached nobody, so it is
                // asked again here. Idempotent, which is what makes it
                // safe on a later reconnection too.
                core.refresh_accounts()
                if (!page.archived) {
                    core.select_account(page.accountId)
                }
                chats.reload()
                // A search typed before the core was up found nothing and
                // had nothing to answer with; this is when it can.
                searchModel.reload()
            }
        }
        // How many profiles there are, which is what decides whether
        // this page offers switching at all. An account list with none
        // left in it is the window's business rather than this page's:
        // it comes in while this page is being replaced or the one above
        // it popped, and a page in the middle of a transition is the one
        // thing on the phone that cannot move the stack. piirit.qml.
        onAccounts_refreshed: page.accountCount = configured_count
        // Failures that would otherwise reach no one.
        onCore_error: page.errorMessage = message
        onIo_started: {
            if (!success) {
                page.errorMessage = error
            }
        }
    }

    Component.onCompleted: {
        core.refresh_accounts()
        // Whichever profile this list shows is the one the app comes
        // back to next time: the core keeps the choice on disk. Said
        // here rather than where the switch is made, because every way
        // to a profile -- resuming, switching, adding one -- ends on
        // this page. The archived list is the same profile's.
        if (!page.archived) {
            core.select_account(page.accountId)
            // And on this side of the core, where the next launch can
            // read it without waiting for the core to start.
            Settings.lastAccountId = page.accountId
            // And the window, which is where a share arrives: it has no
            // page of its own to read the profile off. Behind the check
            // this page already needs for `appWindow`, which a page
            // loaded on its own in a test does not have.
            if (typeof appWindow !== "undefined") {
                appWindow.accountId = page.accountId
            }
        }
    }

    // One flickable for the whole page, owning the pulley.
    //
    // A PullDownMenu draws at the top of the flickable that owns
    // it, and the list starts below the search field -- so a pulley
    // on the list opened below the field, or inside it. It does not
    // scroll: the field has to stay out of a view whose contents
    // change on every keystroke, which is what took the keyboard
    // away mid-search.
    SilicaFlickable {
        id: pulleyHost
        anchors.fill: parent
        // Both, and stated rather than relied on: a Flickable's content
        // is 0 by 0 until told otherwise, and anything anchored to
        // `parent` in here is anchored to that content.
        contentWidth: width
        contentHeight: height

        // Silica builds a pushed page before it starts the transition
        // to it, so the tap sits on a still screen for however long the
        // page and everything it imports take to compile -- which on the
        // group page, with its avatar, its field and its rows, is long
        // enough to read as the app having missed the tap. `animatorPush`
        // is Silica's own answer: the transition starts first and the
        // page is built behind it. Asked for rather than assumed, with
        // the plain push as the fallback, so a Silica without it still
        // opens the page.
        function openPage(url, properties) {
            if (pageStack.animatorPush) {
                pageStack.animatorPush(url, properties)
            } else {
                pageStack.push(url, properties)
            }
        }

        PullDownMenu {
            objectName: "chatListPulley"
            visible: !page.archived
            enabled: !page.archived

            // Every way to something starts here, nearest the list
            // last: the three that make something new are at the bottom,
            // where a short pull reaches them. Settings is the app's own
            // -- a profile's settings are on its row on the profiles page.
            MenuItem {
                objectName: "settingsMenuItem"
                visible: !page.archived
                text: qsTr("Settings")
                // The profile goes in for the one row there that is a
                // profile's: the block list the core keeps per account.
                onClicked: pulleyHost.openPage(Qt.resolvedUrl("SettingsPage.qml"),
                                          { accountId: page.accountId })
            }
            MenuItem {
                objectName: "profilesMenuItem"
                // Not gated on there being more than one: adding a second
                // is itself a reason to open this, and hiding it until
                // there are two leaves no way to make one.
                visible: !page.archived
                text: qsTr("Profiles")
                onClicked: pulleyHost.openPage(Qt.resolvedUrl("ProfilesPage.qml"),
                                          { currentAccountId: page.accountId })
            }
            MenuItem {
                // The archived list is a mode, not a filter, so it is its
                // own page rather than a toggle on this one -- and it does
                // not offer a way further in.
                objectName: "archivedMenuItem"
                visible: !page.archived
                text: qsTr("Archived chats")
                onClicked: pulleyHost.openPage(Qt.resolvedUrl("ChatListPage.qml"), {
                    accountId: page.accountId,
                    archived: true
                })
            }
            MenuItem {
                // Both directions of an invite: this profile's code to
                // show, and someone else's to scan -- which is how a
                // Delta Chat contact is added, since an address alone
                // produces a chat that cannot be encrypted.
                objectName: "qrMenuItem"
                visible: !page.archived
                text: qsTr("QR code")
                onClicked: pulleyHost.openPage(Qt.resolvedUrl("QrPage.qml"),
                                          { accountId: page.accountId })
            }
            MenuItem {
                objectName: "newGroupMenuItem"
                visible: !page.archived
                text: qsTr("New group")
                onClicked: pulleyHost.openPage(Qt.resolvedUrl("NewGroupPage.qml"),
                                          { accountId: page.accountId })
            }
            MenuItem {
                objectName: "newChatMenuItem"
                visible: !page.archived
                text: qsTr("New chat")
                onClicked: pulleyHost.openPage(Qt.resolvedUrl("NewChatPage.qml"),
                                          { accountId: page.accountId })
            }
        }

        // Outside the list, not in its `header`. A header item lives inside
        // the view's flickable, and both plausible explanations for the
        // reported focus loss run through that: Silica hides the input panel
        // when a flickable's content moves, and the narrowing list moves it on
        // every keystroke pause; and a view is a focus scope, which can hand
        // focus to a delegate as rows come and go. Anchored out here the field
        // is in neither story, and it no longer scrolls away mid-search.
        Column {
            id: heading
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }

            PageHeader {
                title: page.archived ? qsTr("Archived") : qsTr("Chats")
            }

            SearchField {
                id: searchField
                objectName: "chatSearchField"
                width: parent.width
                // The ordinary list searches chats, contacts and messages;
                // the archived list is a mode over chats alone.
                // Nothing to search in an empty archive. It comes back
                // the moment something has been typed, or there would be
                // no way to clear the field and get the list back.
                visible: !page.archived || chats.count > 0
                         || searchField.text.length > 0
                // The one word every search field in the app says; what
                // each searches is what the page it is on shows.
                placeholderText: qsTr("Search")
                onTextChanged: searchDebounce.restart()
            }
        }

        SilicaListView {
            id: listView
            objectName: "chatList"
            visible: !page.searching
            anchors {
                top: heading.bottom
                left: parent.left
                right: parent.right
                bottom: banner.top
            }
            // Delegates draw outside the list's own box otherwise, and the
            // banner below is translucent.
            clip: true
            model: chats.rows

            // Pinned chats already sort to the top; this says why they
            // are there. Both headings disappear when one group is
            // empty: a single heading over the whole list says nothing,
            // and "Pinned" over a list with nothing pinned is a lie.
            section {
                property: "is_pinned"
                // Wrapped rather than hidden in place. Silica's
                // SectionHeader keeps its own size and its label is a
                // child of it, so collapsing the header itself left the
                // text drawn over the first row. This collapses a plain
                // Item instead, and clips, so there is nothing left to
                // draw whatever the header does internally.
                delegate: Item {
                    objectName: "chatSectionSlot"
                    width: listView.width
                    height: header.worthSaying ? header.height : 0
                    clip: true

                    SectionHeader {
                        id: header
                        objectName: "chatSection"
                        // No width here. Silica's own is the page width
                        // less a margin at each side, with the text
                        // right-aligned in it -- and its x is that left
                        // margin. Assigning the full parent width kept
                        // the x and pushed the right edge, and the text
                        // on it, a whole margin off the screen.
                        // Both headings or neither: one heading over the
                        // whole list says nothing, and "Pinned" over a
                        // list with nothing pinned is a lie.
                        readonly property bool worthSaying:
                            chats.pinned_count > 0 && chats.unpinned_count > 0
                        visible: worthSaying
                        text: section === "true" ? qsTr("Pinned")
                                                 : qsTr("Other chats")
                    }
                }
            }

            // Nothing in here applies to the archived list -- no profile
            // switch, no way further in, no new chat -- so the pulley itself
            // goes rather than opening onto an empty menu.

            delegate: ListItem {
                id: delegateRoot
                contentHeight: body.height

                /// This chat is on its way out.
                readonly property bool doomed: doomedChats.pending(model.chat_id)

                /// Silica's own countdown, drawn over the chat. The
                /// deletion is not its business -- that belongs to
                /// `doomedChats`, because a remorse item lives in the
                /// row it covers and this list reorders under its rows
                /// whenever a message arrives. So it draws, and reports
                /// the tap.
                function raiseRemorse() {
                    //: What Silica's countdown says it is doing, over
                    //: a chat the reader has asked to delete.
                    remorse.execute(
                        body, qsTr("Deleting"), function() {},
                        doomedChats.countdownFor(model.chat_id))
                }

                RemorseItem {
                    id: remorse
                    objectName: "chatRemorse"
                    onCanceled: doomedChats.spare(model.chat_id)
                }

                // A row rebuilt mid-wait -- scrolled past, or moved by a
                // message arriving -- comes back with no countdown on
                // it. Put it up again with what is left of the wait.
                Component.onCompleted: {
                    if (delegateRoot.doomed) {
                        delegateRoot.raiseRemorse()
                    }
                }

                ChatListDelegate {
                    id: body
                    // The remorse covering this does the fading, with
                    // its own `opacity: 0.0` on what it was handed.
                    // Hiding it here would be wrong: that takes the
                    // children's `visible` with it, and a row whose
                    // height reads `visible` collapses under the
                    // countdown drawn over it.
                    enabled: !delegateRoot.doomed
                    width: parent.width
                    chatName: model.name
                    preview: model.preview
                    previewSender: model.preview_sender
                    unreadCount: model.unread_count
                    lastUpdated: model.last_updated
                    isEncrypted: model.is_encrypted
                    isPinned: model.is_pinned
                    isMuted: model.is_muted
                    isContactRequest: model.is_contact_request
                    chatColor: model.color
                    avatarPath: model.avatar_path
                    summaryState: model.summary_state
                }

                menu: ContextMenu {
                    MenuItem {
                        objectName: "markReadItem"
                        visible: !page.archived && model.unread_count > 0
                        text: qsTr("Mark as read")
                        onClicked: chats.mark_read(model.chat_id)
                    }
                    // The other way: one unread message back on a chat
                    // already read, so it stands out until it is opened.
                    // The core puts its last incoming message back to
                    // unread, so a chat with only the reader's own
                    // messages in it stays as it is.
                    MenuItem {
                        objectName: "markUnreadItem"
                        visible: !page.archived && model.unread_count === 0
                        text: qsTr("Mark as unread")
                        onClicked: chats.mark_unread(model.chat_id)
                    }
                    MenuItem {
                        objectName: "pinItem"
                        visible: !page.archived
                        text: model.is_pinned ? qsTr("Unpin") : qsTr("Pin")
                        onClicked: chats.set_pinned(model.chat_id, !model.is_pinned)
                    }
                    MenuItem {
                        objectName: "muteItem"
                        visible: !page.archived
                        text: model.is_muted ? qsTr("Unmute") : qsTr("Mute")
                        onClicked: chats.set_muted(model.chat_id, !model.is_muted)
                    }
                    // A request is not an ordinary chat: until it is
                    // accepted the sender cannot be replied to, and the only
                    // useful answers are yes and no.
                    MenuItem {
                        objectName: "acceptItem"
                        visible: !page.archived && model.is_contact_request
                        text: qsTr("Accept")
                        onClicked: chats.accept_chat(model.chat_id)
                    }
                    MenuItem {
                        objectName: "blockItem"
                        visible: !page.archived && model.is_contact_request
                        text: qsTr("Block")
                        onClicked: chats.block_chat(model.chat_id)
                    }
                    MenuItem {
                        objectName: "archiveItem"
                        visible: !page.archived && !model.is_contact_request
                        text: qsTr("Archive")
                        onClicked: chats.archive(model.chat_id)
                    }
                    // The way back out. Without it an archived chat could only
                    // be recovered by someone sending a message to it.
                    MenuItem {
                        objectName: "unarchiveItem"
                        visible: page.archived
                        text: qsTr("Unarchive")
                        onClicked: chats.unarchive(model.chat_id)
                    }
                    MenuItem {
                        objectName: "deleteItem"
                        text: qsTr("Delete")
                        // The page is told, not this row: a message
                        // arriving moves this chat up the list, which is
                        // a remove and an insert, and the row this menu
                        // belongs to is destroyed. A wait living on it
                        // would go too.
                        onClicked: {
                            doomedChats.ask(model.chat_id)
                            delegateRoot.raiseRemorse()
                        }
                    }
                }

                // A tap opens the chat. One waiting to go is covered by
                // the remorse, which takes the tap itself and calls the
                // deletion off.
                onClicked: page.openChat(model.chat_id, model.name, 0)
            }

            ViewPlaceholder {
                objectName: "chatListPlaceholder"
                // Not until the core has answered: see `chatsLoaded`.
                enabled: page.chatsLoaded && chats.count === 0
                text: page.archived ? qsTr("No archived chats")
                                    : qsTr("No chats yet")
                // Nothing here makes an archived chat: a chat is archived
                // from the ordinary list, not started in this one.
                hintText: page.archived ? "" : qsTr("Pull down to start one")
            }
        }

        SearchResultsList {
            id: resultsView
            objectName: "searchResults"
            visible: page.searching
            search: searchModel
            anchors {
                top: heading.bottom
                left: parent.left
                right: parent.right
                bottom: banner.top
            }
            onChatActivated: page.openChat(chatId, chatName, messageId)
            // No chat exists yet; the model answers on chat_ready. The name
            // is kept here because that answer carries only an id.
            onContactActivated: {
                page.pendingChatName = contactName
                searchModel.open_chat_with(contactId)
            }
        }

        // The tap has to show something while the chat loads, or a slow
        // one reads as a tap that missed.
        BusyIndicator {
            objectName: "openingChat"
            anchors.centerIn: parent
            running: prefetch.loading
            size: BusyIndicatorSize.Large
        }

        Banner {
            id: banner
            objectName: "errorBanner"
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            // The core's own state outranks whatever failed before it, and a
            // page opened after it went away never saw the transition -- so
            // read the status rather than waiting for it to change.
            text: page.coreStatusMessage.length > 0
                  ? page.coreStatusMessage : page.errorMessage
            // Neither clears itself: reconnecting ends when the status says so,
            // and stopped does not end at all.
            timeout: page.coreStatusMessage.length > 0 ? 0 : 8
            onDismissed: page.errorMessage = ""
        }
    }
}
