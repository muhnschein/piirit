import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0
import "../js/QuickActions.js" as QuickActions

Page {
    id: page

    property int accountId
    /// Archived chats, not ordinary: a mode, not filter.
    property bool archived: false
    /// Attached right of a finished page (backup, handover). Stack bottom stays
    /// window's.
    property bool attached: false
    /// Configured account count. Decides if switching is offered.
    property int accountCount: 0

    // Not bound to field: core round trip per keystroke, only last answer
    // wanted. Same interval as contact search.
    Timer {
        id: searchDebounce
        interval: 250
        onTriggered: {
            // Archived list filters itself; ordinary list searches all three
            // kinds, grouped.
            if (page.archived) {
                chats.query = searchField.text
            } else {
                searchModel.query = searchField.text
            }
        }
    }

    /// Field replaces Search and Unread pills. Archived list always has it.
    property bool searchOpen: false

    function openSearch() {
        page.searchOpen = true
        searchField.forceActiveFocus()
    }

    function closeSearch() {
        searchField.text = ""
        page.searchOpen = false
        searchField.focus = false
    }

    /// Enter hides keyboard, keeps search.
    function dismissKeyboard() {
        searchField.focus = false
    }

    /// Results shown instead of chat list. Read from field: swap on keystroke.
    readonly property bool searching:
        !page.archived && searchField.text.trim().length > 0

    // Load chat before its page exists: messages in hand, no empty flash.
    ChatPrefetch {
        id: prefetch
        objectName: "prefetch"
        account_id: page.accountId
        onReady: page.openLoadedChat(chat_id)
    }

    /// Pending tap target during prefetch: answer carries only an id.
    property string pendingChatName: ""
    property int pendingMessageId: 0

    function openChat(chatId, chatName, messageId) {
        page.pendingChatName = chatName
        page.pendingMessageId = messageId
        // Told before push: arrivals during transition not announced.
        notifier.viewingChatId = chatId
        // Search-found message goes in too: page opens on it, not newest.
        prefetch.start(chatId, messageId ? messageId : 0)
    }

    function openLoadedChat(chatId) {
        // Only while on screen: a Profiles visit during load cancels the chat.
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
        // Contact result has no chat until tapped.
        onChat_ready: page.openChat(chat_id, page.pendingChatName, 0)
    }

    ChatList {
        id: chats
        objectName: "chats"
        account_id: page.accountId
        archived: page.archived
        // Reply to reader's own message gets through muted group, if wanted.
        // `=== true`: dconf undefined before read.
        notify_mentions: Settings.mentionNotifications === true
        onError: page.errorMessage = message
    }

    // Here: list owns arrival logic; page knows which chat reader is in.
    Notifier {
        id: notifier
        objectName: "notifier"
        // Off: nothing announced, shown ones taken down; model still reports
        // arrivals.
        enabled: Settings.notificationsEnabled === true
        detail: Settings.notificationDetail
        // Notification tap: back to list, then into chat.
        onOpenRequested: page.showChat(chatId)
    }

    /// Core said what is in profile. Empty means no chats or no answer yet
    /// (true on resume): no "No chats yet" until set.
    property bool chatsLoaded: false

    Connections {
        target: chats
        onMessage_arrived: notifier.arrived(chat_id, chat_name, sender, preview)
        onMessage_restated: notifier.restated(chat_id, chat_name, sender, preview, unread)
        // Emitted once rows set.
        onRows_changed: page.chatsLoaded = true
    }

    /// Bring list to front, as an outside tap does: window raised (lipstick only
    /// calls app, doesn't raise it), pages above popped. Test pages: no window.
    ///
    /// Not during a call or core-busy work (backup, profile move;
    /// `quickActionsPaused`): app comes forward onto it, returns false.
    /// Popping would drop call sound, or leave core nothing to report to.
    function comeForward() {
        if (typeof appWindow !== "undefined") {
            appWindow.activate()
            if (appWindow.callBusy === true) {
                appWindow.showCall()
                return false
            }
            if (appWindow.quickActionsPaused === true) {
                return false
            }
        }
        if (pageStack.currentPage !== page) {
            pageStack.pop(page, PageStackAction.Immediate)
        }
        return true
    }

    /// Open a chat from outside: notification tapped.
    function showChat(chatId) {
        if (!page.comeForward()) {
            return
        }
        page.openChat(chatId, notifier.nameOf(chatId), 0)
    }

    /// Cover quick action (piirit.qml). `kind`; chat kinds need `accountId`,
    /// `chatId`.
    /// `sendNow`: camera capture sent once chat picked.
    ///
    /// Brought forward over open pages. Chat looked up first: deleted chat
    /// reported gone. Core away: action ends at list.
    function quickAction(kind, accountId, chatId, sendNow) {
        if (!page.comeForward()) {
            return
        }
        // Earlier lookup no longer wanted.
        page.quickChatPending = false
        if (QuickActions.isCapture(kind)) {
            page.capture(QuickActions.captures[kind], sendNow === true)
        } else if (kind === "search") {
            searchField.text = ""
            page.searchWanted = true
            page.takeSearch()
        } else if (kind === "qr" || kind === "scan") {
            pulleyHost.openPage(Qt.resolvedUrl("QrPage.qml"), {
                accountId: page.accountId,
                mode: kind === "scan" ? 1 : 0
            })
        } else if (kind === "profiles") {
            // As pull-down's Profiles does.
            pulleyHost.openPage(Qt.resolvedUrl("ProfilesPage.qml"),
                                { currentAccountId: page.accountId })
        } else if (kind === "chat" && core.status === "ready") {
            page.quickChatPending = true
            // Through 0: same chat asked twice still counts as a change.
            quickChat.chat_id = 0
            quickChat.account_id = accountId
            quickChat.chat_id = chatId
        }
    }

    /// Quick action's chat being looked up.
    property bool quickChatPending: false

    /// Camera quick action: capture, then chat to send to, then that chat with
    /// capture on bar. `sendNow` sends at once; else waits for caption and send
    /// button, as a share does (piirit.qml).
    ///
    /// Pages replace, not stack: back from chat returns here. Pushed, not
    /// `pulleyHost`: camera's answer is connected to, and animated push returns
    /// no page.
    function capture(how, sendNow) {
        // Earlier capture is dead now.
        page.dropCapture()
        var camera = pageStack.push(Qt.resolvedUrl("CapturePage.qml"), {
            mode: how.mode,
            front: how.front,
            closeOnPick: false
        })
        // Null if page failed to load (no QtMultimedia): costs this action only.
        if (!camera) {
            return
        }
        camera.picked.connect(function (path) {
            // Finished after camera left (backed out, or call over it): discard;
            // page in camera's place isn't camera's to replace.
            if (pageStack.currentPage !== camera || pageStack.busy) {
                captures.discard(path)
                return
            }
            page.pickChatFor(path, sendNow)
        })
    }

    /// Ask which chat a capture goes to, in the camera's place.
    function pickChatFor(path, sendNow) {
        page.unsentCapture = path
        var picker = pageStack.replace(Qt.resolvedUrl("ChatPickerPage.qml"), {
            accountId: page.accountId,
            closeOnPick: false,
            //: Over the list of chats, when picking the one to send a
            //: picture or a video just taken to.
            title: qsTr("Send to"),
            emptyText: qsTr("No chats yet")
        })
        if (!picker) {
            page.dropCapture()
            return
        }
        picker.chatPicked.connect(function (chatId, chatName) {
            // Chat owns capture; discarded once sent.
            page.unsentCapture = ""
            pageStack.replace(Qt.resolvedUrl("ConversationPage.qml"), {
                accountId: page.accountId,
                chatId: chatId,
                chatName: chatName,
                sharedFile: path,
                sendShared: sendNow
            })
        })
    }

    /// Capture with no chat picked yet, "" for none. Backing out discards it.
    property string unsentCapture: ""

    function dropCapture() {
        if (page.unsentCapture.length === 0) {
            return
        }
        captures.discard(page.unsentCapture)
        page.unsentCapture = ""
    }

    Captures {
        id: captures
        objectName: "captures"
    }

    // Lookup failure emits loaded then error: open a turn later, after the
    // error.
    ChatInfo {
        id: quickChat
        objectName: "quickChat"
        onLoaded_changed: {
            if (quickChat.loaded && page.quickChatPending) {
                quickChatFound.restart()
            }
        }
        onError: {
            if (page.quickChatPending) {
                page.quickChatPending = false
                //: A quick action on the cover was tapped, and the chat it
                //: was set up to open has been deleted since.
                page.errorMessage = qsTr("That chat no longer exists.")
            }
        }
    }

    Timer {
        id: quickChatFound
        interval: 0
        onTriggered: {
            if (!page.quickChatPending) {
                return
            }
            page.quickChatPending = false
            // Only while on screen: a call's page over this one would be
            // replaced.
            if (page.status !== PageStatus.Active) {
                return
            }
            if (quickChat.account_id === page.accountId) {
                page.openChat(quickChat.chat_id, quickChat.name, 0)
                return
            }
            // Other profile's chat: its list replaces whole stack (as profiles
            // page switches); chat opened once list on screen.
            pageStack.replaceAbove(null, Qt.resolvedUrl("ChatListPage.qml"), {
                accountId: quickChat.account_id,
                arrivingChatId: quickChat.chat_id,
                arrivingChatName: quickChat.name
            })
        }
    }

    /// Chat to open once list on screen; quick action for another profile set
    /// it here. 0 for none.
    property int arrivingChatId: 0
    property string arrivingChatName: ""

    /// Search field takes keyboard once list on screen (quick action asked for
    /// it).
    property bool searchWanted: false
    readonly property bool appActive: Qt.application.state === Qt.ApplicationActive
    onAppActiveChanged: {
        page.takeSearch()
        // Chats read under unread filter drop once app was away; kept only so
        // returning from one didn't move list.
        if (page.appActive) {
            chats.forget_read()
        }
    }

    function takeSearch() {
        if (!page.searchWanted || page.status !== PageStatus.Active || !page.appActive) {
            return
        }
        page.searchWanted = false
        page.openSearch()
    }

    // Back on list: no chat being read. Leaving: pending deletes go now (asked
    // to go, then left = still wants it gone).
    onStatusChanged: {
        if (status === PageStatus.Active) {
            notifier.viewingChatId = 0
            page.dropCapture()
            if (page.arrivingChatId !== 0) {
                var arriving = page.arrivingChatId
                page.arrivingChatId = 0
                page.openChat(arriving, page.arrivingChatName, 0)
            }
            page.takeSearch()
        } else if (status === PageStatus.Deactivating) {
            doomedChats.flush()
        }
    }

    /// Wait before chat goes, ms. Tests set low.
    property alias pendingDelay: doomedChats.delay

    /// Chats asked to delete, in undo wait. Not `remorseAction`: arrival moves
    /// chat (remove + insert), destroying row and its wait. See PendingRemoval.
    PendingRemoval {
        id: doomedChats
        onRemove: chats.delete_chat(id)
    }

    property string errorMessage: ""
    // Three states: core going away is handled, so no "restart Piirit" banner.
    readonly property string coreStatusMessage:
        core.status === "reconnecting"
        ? qsTr("Lost the connection to the Delta Chat core. Reconnecting…")
        : core.status === "stopped"
          ? qsTr("Lost the connection to the Delta Chat core. Restart Piirit.")
          : ""

    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml. Model ignores other
        // accounts' events itself.
        onCore_event: chats.handle_event(context_id, kind, payload_json)
        onStatus_changed: {
            if (core.status === "ready") {
                // Resume: first page made before core up, so its request was
                // lost. Ask again; idempotent, so safe on reconnect too.
                core.refresh_accounts()
                if (!page.archived) {
                    core.select_account(page.accountId)
                }
                chats.reload()
                // Search typed before core up is answered now.
                searchModel.reload()
            }
        }
        // Profile count decides switching. Empty account list is window's
        // problem: it lands mid-transition, when a page can't move stack.
        // piirit.qml.
        onAccounts_refreshed: page.accountCount = configured_count
        // Failures that would reach no one.
        onCore_error: page.errorMessage = message
        onIo_started: {
            if (!success) {
                page.errorMessage = error
            }
        }
    }

    Component.onCompleted: {
        core.refresh_accounts()
        // Profile shown is what next launch returns to (core keeps it on disk);
        // every route to a profile ends here. Archived list is same profile.
        if (!page.archived) {
            core.select_account(page.accountId)
            // Next launch reads it without waiting for core.
            Settings.lastAccountId = page.accountId
            // Window too: shares arrive there, with no page to read profile off.
            // Guarded: test pages have no `appWindow`.
            if (typeof appWindow !== "undefined") {
                appWindow.accountId = page.accountId
                // Cover quick actions land here, not on a list beside a backup
                // (gone on swipe back, leaving window none).
                if (!page.attached) {
                    appWindow.chatList = page
                }
            }
        }
    }

    // One flickable owns pulley: PullDownMenu draws at top of its flickable,
    // and list starts below field, so pulley on list opened below field.
    // Does not scroll: field must stay out of a view that changes every
    // keystroke (took keyboard away mid-search).
    SilicaFlickable {
        id: pulleyHost
        anchors.fill: parent
        // Flickable content is 0x0 until told; parent anchors go to content.
        contentWidth: width
        contentHeight: height

        // Silica builds pushed page before transition: slow pages (group page)
        // read as missed tap. animatorPush starts transition first, builds
        // behind. Plain push as fallback.
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

            // Every route starts here, nearest list last: create items at
            // bottom, short pull. Settings is the app's; profile settings are on
            // profiles page rows.
            MenuItem {
                objectName: "settingsMenuItem"
                visible: !page.archived
                text: qsTr("Settings")
                // Profile goes in for its row: block list kept per account.
                onClicked: pulleyHost.openPage(Qt.resolvedUrl("SettingsPage.qml"),
                                          { accountId: page.accountId })
            }
            MenuItem {
                objectName: "profilesMenuItem"
                // Not gated on >1 profile: adding a second is a reason to open
                // this.
                visible: !page.archived
                text: qsTr("Profiles")
                onClicked: pulleyHost.openPage(Qt.resolvedUrl("ProfilesPage.qml"),
                                          { currentAccountId: page.accountId })
            }
            MenuItem {
                // Archived list is a mode with its own page; no further way in.
                objectName: "archivedMenuItem"
                visible: !page.archived
                text: qsTr("Archived chats")
                onClicked: pulleyHost.openPage(Qt.resolvedUrl("ChatListPage.qml"), {
                    accountId: page.accountId,
                    archived: true
                })
            }
            MenuItem {
                // Both invite directions: show own code, scan theirs (address
                // alone can't encrypt).
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

        // Outside list, not in header: header lives in view's flickable, where
        // Silica hides keyboard when content moves (every keystroke) and view's
        // focus scope can hand focus to a delegate. Lost focus mid-search.
        Column {
            id: heading
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }

            PageHeader {
                objectName: "chatListHeader"
                title: page.archived ? qsTr("Archived") : qsTr("Chats")
                // Nothing under title while filter on: lit pill says so, and a
                // description would push search row under the finger.
            }

            // Ordinary list: Search and Unread pills; field cross-fades in once
            // Search tapped, out when closed or left empty. Archived list: field
            // only.
            Item {
                width: parent.width
                height: searchField.height

                SearchField {
                    id: searchField
                    objectName: "chatSearchField"
                    width: parent.width
                    // Nothing to search in empty archive. Field returns on
                    // typing, so it can be cleared.
                    visible: page.archived
                             ? chats.count > 0 || searchField.text.length > 0
                             : page.searchOpen || opacity > 0
                    opacity: page.archived || page.searchOpen ? 1 : 0
                    enabled: page.archived || page.searchOpen
                    Behavior on opacity { NumberAnimation { duration: 200 } }
                    // Pill's word and magnifier at pill size and place, so one
                    // fades into the other.
                    font.pixelSize: Theme.fontSizeMedium
                    textLeftMargin: Theme.horizontalPageMargin + Theme.paddingMedium
                                    + Theme.iconSizeSmallPlus + Theme.paddingSmall
                    // Silica's magnifier replaced by one beside field; slot
                    // stays empty.
                    leftItem: Item {}
                    // Empty field offers to go away, not clear.
                    canHide: !page.archived
                    onHideClicked: page.closeSearch()
                    // Enter closes keyboard, keeps search. EnterKey attached
                    // prop breaks test stubs.
                    Keys.onReturnPressed: page.dismissKeyboard()
                    Keys.onEnterPressed: page.dismissKeyboard()
                    // Same word every search field uses.
                    placeholderText: qsTr("Search")
                    onTextChanged: {
                        searchDebounce.restart()
                        if (searchField.text.length > 0) {
                            page.searchOpen = !page.archived
                        } else if (!searchField.activeFocus) {
                            page.searchOpen = false
                        }
                    }
                    // Left empty: row goes back to pills.
                    onActiveFocusChanged: {
                        if (!searchField.activeFocus
                                && searchField.text.trim().length === 0) {
                            page.closeSearch()
                        }
                    }
                }

                // Magnifier drawn over field, not leftItem (Silica places that
                // off-pill).
                Image {
                    x: Theme.horizontalPageMargin + Theme.paddingMedium
                    anchors.verticalCenter: parent.verticalCenter
                    visible: searchField.visible
                    opacity: searchField.opacity
                    width: Theme.iconSizeSmallPlus
                    height: Theme.iconSizeSmallPlus
                    sourceSize.width: Theme.iconSizeSmallPlus
                    sourceSize.height: Theme.iconSizeSmallPlus
                    source: "image://theme/icon-m-search?"
                            + (searchField.activeFocus ? Theme.highlightColor
                                                       : Theme.primaryColor)
                }

                Row {
                    objectName: "chatListPills"
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: Theme.paddingMedium
                    visible: !page.archived && (!page.searchOpen || opacity > 0)
                    opacity: page.searchOpen ? 0 : 1
                    enabled: !page.searchOpen
                    Behavior on opacity { NumberAnimation { duration: 200 } }

                    PillButton {
                        objectName: "chatSearchPill"
                        icon: "image://theme/icon-m-search"
                        text: qsTr("Search")
                        onClicked: page.openSearch()
                    }

                    // Beside search: one tap away, as reference clients. Goes
                    // with Search pill while open: search covers every chat.
                    PillButton {
                        id: unreadFilter
                        objectName: "unreadFilterButton"
                        //: Turns the chat list's filter to unread chats on
                        //: and off.
                        text: qsTr("Unread")
                        checked: chats.unread_only
                        onClicked: chats.unread_only = !chats.unread_only
                    }
                }
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
            // Delegates otherwise draw outside list box; banner is translucent.
            clip: true
            model: chats.rows

            // Pinned chats sort top; headings say why. Shown only when both
            // groups non-empty: "Pinned" over nothing pinned is a lie.
            section {
                property: "is_pinned"
                // Wrapped, not hidden: SectionHeader keeps its size; collapsing
                // it left text over first row.
                delegate: Item {
                    objectName: "chatSectionSlot"
                    width: listView.width
                    height: header.worthSaying ? header.height : 0
                    clip: true

                    SectionHeader {
                        id: header
                        objectName: "chatSection"
                        // No width set: Silica's is page width less margins,
                        // x = left margin; full width pushed text off-screen.
                        readonly property bool worthSaying:
                            chats.pinned_count > 0 && chats.unpinned_count > 0
                        visible: worthSaying
                        text: section === "true" ? qsTr("Pinned")
                                                 : qsTr("Other chats")
                    }
                }
            }

            // Nothing applies to archived list: no pulley, rather than an empty
            // menu.

            delegate: ListItem {
                id: delegateRoot
                contentHeight: body.height

                /// Chat on its way out.
                readonly property bool doomed: doomedChats.pending(model.chat_id)

                /// Silica's countdown over chat; only draws, reports tap.
                /// Deletion belongs to `doomedChats`: remorse lives in its row,
                /// and list reorders rows on arrivals.
                function raiseRemorse() {
                    remorse.active = true
                    //: What Silica's countdown says it is doing, over
                    //: a chat the reader has asked to delete.
                    remorse.item.execute(
                        body, qsTr("Deleting"), function() {},
                        doomedChats.countdownFor(model.chat_id))
                }

                // Built on first delete, not per row: a dozen items per chat
                // for a rare tap.
                Loader {
                    id: remorse
                    active: false
                    sourceComponent: RemorseItem {
                        objectName: "chatRemorse"
                        onCanceled: doomedChats.spare(model.chat_id)
                    }
                }

                // Row rebuilt mid-wait comes back bare: put countdown back with
                // what's left.
                Component.onCompleted: {
                    if (delegateRoot.doomed) {
                        delegateRoot.raiseRemorse()
                    }
                }

                ChatListDelegate {
                    id: body
                    // Remorse does the fading. Hiding here would hide children
                    // too, and row height reads `visible`: row collapses.
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

                // Component, built on first open: nine items per chat is waste.
                menu: Component {
                    ContextMenu {
                        MenuItem {
                            objectName: "markReadItem"
                            visible: !page.archived && model.unread_count > 0
                            text: qsTr("Mark as read")
                            onClicked: chats.mark_read(model.chat_id)
                        }
                        // One unread message back on a read chat: stands out
                        // until opened. Core re-marks last incoming, so chat with
                        // only own messages unchanged.
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
                        // Request: can't reply until accepted, so only yes or
                        // no.
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
                        // Way back out of archive; otherwise only a message
                        // brings it back.
                        MenuItem {
                            objectName: "unarchiveItem"
                            visible: page.archived
                            text: qsTr("Unarchive")
                            onClicked: chats.unarchive(model.chat_id)
                        }
                        MenuItem {
                            objectName: "deleteItem"
                            text: qsTr("Delete")
                            // Page told, not row: arrivals remove and insert
                            // rows, destroying any wait on them.
                            onClicked: {
                                doomedChats.ask(model.chat_id)
                                delegateRoot.raiseRemorse()
                            }
                        }
                    }
                }

                // Tap opens chat. One waiting to go is cancelled by the remorse
                // tap.
                onClicked: page.openChat(model.chat_id, model.name, 0)
            }

            ViewPlaceholder {
                objectName: "chatListPlaceholder"
                // Not until core answered: see `chatsLoaded`.
                enabled: page.chatsLoaded && chats.count === 0
                text: page.archived ? qsTr("No archived chats")
                                    //: The unread filter is on and every
                                    //: chat has been read.
                                    : chats.unread_only ? qsTr("No unread chats")
                                                        : qsTr("No chats yet")
                // Archived chats are made from the ordinary list, not here.
                hintText: page.archived ? ""
                                        //: %1 is the label of the unread
                                        //: filter, a button at the right of
                                        //: the search field.
                                        : chats.unread_only ? qsTr("Tap %1 to show all chats").arg(unreadFilter.text)
                                                            : qsTr("Pull down to start one")
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
            // No chat yet; name kept here since answer carries only id.
            onContactActivated: {
                page.pendingChatName = contactName
                searchModel.open_chat_with(contactId)
            }
        }

        // Tap shows something while chat loads, or a slow load reads as missed.
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
            // Core state outranks earlier failure. Read status: page opened
            // after core change never saw transition.
            text: page.coreStatusMessage.length > 0
                  ? page.coreStatusMessage : page.errorMessage
            // Reconnecting ends when status says so; stopped never ends.
            timeout: page.coreStatusMessage.length > 0 ? 0 : 8
            onDismissed: page.errorMessage = ""
        }
    }
}
