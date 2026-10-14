import QtQuick 2.0
import Sailfish.Silica 1.0
import "pages"
import "cover"
import "components"
import "js/QuickActions.js" as QuickActions
import "js/StockStrings.js" as StockStrings

ApplicationWindow {
    id: appWindow

    /// Profile to open on. 0: none yet.
    ///
    /// Read once: later dconf writes must not reach the screen.
    property int resumeAccountId: appWindow.rememberedProfile()

    /// dconf number. `> 0`: dconf gives `undefined` until read.
    function rememberedProfile() {
        return Settings.lastAccountId > 0 ? Settings.lastAccountId : 0
    }

    // Used phone opens on chat list. Welcome page never made (no put-up/animate-out).
    initialPage: appWindow.resumeAccountId > 0 ? resumedChats : firstScreen

    Component {
        id: firstScreen
        WelcomePage {}
    }
    Component {
        id: resumedChats
        ChatListPage { accountId: appWindow.resumeAccountId }
    }

    // Cover quick actions tapped here. During call, cover shows call + own action.
    cover: Component {
        CoverPage {
            quickActionsAllowed: appWindow.chatList !== null
                                 && !appWindow.quickActionsPaused
            call: callCenter.call
            onQuickAction: appWindow.quickAction(side)
        }
    }

    /// Chat list app is on (quick action target). Null before first, after last profile.
    property Item chatList: null

    /// Page up that quick actions must not leave (backup, profile made/moved/joined,
    /// code shown or read). Each such page sets `pausesQuickActions`.
    readonly property bool quickActionsPaused: {
        // Re-run when stack moves.
        var depth = pageStack.depth
        for (var shown = pageStack.currentPage; shown;
             shown = pageStack.previousPage(shown)) {
            if (shown.pausesQuickActions === true) {
                return true
            }
        }
        return false
    }

    /// Quick action on cover: app opens, chat list does rest, like a notification.
    function quickAction(side) {
        var action = QuickActions.shown(Settings, side)
        if (appWindow.chatList === null || action.kind === "") {
            return
        }
        appWindow.chatList.quickAction(action.kind, action.accountId, action.chatId,
                                       action.sendNow)
    }

    /// Profile app is on. Shares land in window; 0 = none yet.
    property int accountId: 0

    /// Share arrived: ask which chat, then open it with file on attachment bar, text
    /// in field.
    function shareInto(filePath, text) {
        if (appWindow.accountId === 0) {
            return
        }
        var picker = pageStack.push(
            Qt.resolvedUrl("pages/ChatPickerPage.qml"),
            { accountId: appWindow.accountId, closeOnPick: false })
        if (!picker) {
            return
        }
        picker.chatPicked.connect(function (chatId, chatName) {
            appWindow.openShared(chatId, chatName, filePath, text)
        })
    }

    /// Open chat a share pointed at. Replaces picker: back goes to before the share.
    function openShared(chatId, chatName, filePath, text) {
        pageStack.replace(Qt.resolvedUrl("pages/ConversationPage.qml"), {
            accountId: appWindow.accountId,
            chatId: chatId,
            chatName: chatName,
            sharedFile: filePath,
            sharedText: text
        })
    }

    // Share sheet. Loaded, not declared: `Sailfish.Share` only resolves on releases that ship it,
    // and a failed import would take down the window.
    // Background, off first frame. See qml/share/ShareTarget.qml.
    Loader {
        id: shareTarget
        objectName: "shareTarget"
        asynchronous: true
        source: Qt.resolvedUrl("share/ShareTarget.qml")
    }

    // Three core settings; each applies to all profiles, follows its key.
    Binding {
        target: core
        property: "download_limit"
        value: Settings.downloadLimit
    }
    Binding {
        target: core
        property: "media_quality"
        value: Settings.mediaQuality
    }
    Binding {
        target: core
        property: "delete_device_after"
        value: Settings.deleteDeviceAfter
    }
    // Calls ring: nobody if ringing off, else contacts (core 2 / 1). Non-ringers' calls kept.
    // Only reader's choice writes 2: key travels with profile (backup, second device).
    Binding {
        target: core
        property: "who_can_call_me"
        value: Settings.callsRing === false ? 2 : 1
    }

    /// App's one call, owned by no page. See components/CallCenter.qml.
    CallCenter {
        id: callCenter
        objectName: "callCenter"
        stack: pageStack
        onRaise: appWindow.activate()
        // Tapped missed-call notification: open its chat, like a quick action.
        onChatRequested: {
            if (appWindow.chatList !== null) {
                appWindow.chatList.quickAction("chat", accountId, chatId)
            }
        }
    }

    /// Call under way: notifications and quick actions keep its page up.
    readonly property bool callBusy: callCenter.busy

    /// Call a chat; camera on for video. False if already in a call.
    function placeCall(accountId, chatId, video) {
        return callCenter.place(accountId, chatId, video === true)
    }

    /// Bring call to front.
    function showCall() {
        callCenter.show()
    }

    /// Take up ringing call from its chat row.
    function pickUpCall(accountId, chatId, messageId) {
        callCenter.pickUp(accountId, chatId, messageId)
    }

    /// IO asked for. Once, for app's life.
    property bool askedForIo: false

    /// Whether this run ever had a profile.
    ///
    /// Tells empty lists apart: no profile yet (welcome page, don't resend),
    /// or last profile gone (chat list on account core lacks; taps answer "account not found").
    property bool hadProfile: false

    /// Way back to welcome page, held until stack takes it (last profile leaves mid-pop).
    PendingNavigation {
        id: welcomeAgain
        stack: pageStack
    }

    /// Way to another profile's chats, held the same way.
    PendingNavigation {
        id: otherProfile
        stack: pageStack
    }

    /// Move window off profiles core lacks, onto `resumeAccountId`.
    ///
    /// Deleted list profile stayed on list (and dconf): pages said "account not found".
    /// Runs each refresh, so a stale or elsewhere-deleted profile is left on first listing.
    function leaveGoneProfiles(resumeAccountId) {
        // Quick action's chat went with its profile: none, cover can't open it.
        var sides = ["quickActionLeft", "quickActionRight"]
        for (var i = 0; i < sides.length; i++) {
            var prefix = sides[i]
            if (Settings[prefix] === "chat" && Settings[prefix + "Account"] > 0
                    && !core.is_configured_account(Settings[prefix + "Account"])) {
                Settings[prefix] = ""
                Settings[prefix + "Account"] = 0
                Settings[prefix + "Chat"] = 0
            }
        }
        if (appWindow.accountId === 0 || resumeAccountId === 0
                || core.is_configured_account(appWindow.accountId)) {
            return
        }
        // Now, not on new list: app may close first, next launch must not resume onto it.
        appWindow.accountId = resumeAccountId
        Settings.lastAccountId = resumeAccountId
        otherProfile.replaceAbove(null, Qt.resolvedUrl("pages/ChatListPage.qml"),
                                  { accountId: resumeAccountId })
    }

    // Last profile gone: welcome move lives here, not on pages (swipe destroys profiles page,
    // chat list mid-transition: move ran twice or never).
    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onAccounts_refreshed: {
            if (configured_count > 0) {
                appWindow.hadProfile = true
                appWindow.leaveGoneProfiles(resume_account_id)
                return
            }
            if (!appWindow.hadProfile) {
                return
            }
            appWindow.hadProfile = false
            // Nothing to share into or come back to.
            appWindow.accountId = 0
            Settings.lastAccountId = 0
            welcomeAgain.replaceAbove(null,
                                      Qt.resolvedUrl("pages/WelcomePage.qml"),
                                      {})
        }
    }

    // IO belongs to window: resuming onto chat list never shows welcome page. All profiles get IO.
    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onStatus_changed: {
            if (core.status !== "ready") {
                return
            }
            // Network gone before core up: nothing to start. Marked asked-for and stopped;
            // `resumeIo` reads that. Core dying mid-outage: `pauseIo` guard makes this no-op.
            if (networkWatch.lost) {
                appWindow.askedForIo = true
                appWindow.pauseIo()
                return
            }
            if (!appWindow.askedForIo) {
                appWindow.askedForIo = true
                core.start_all_account_io()
            }
        }
    }

    /// Whether app is in front. Property: bindings notice when it turns true again.
    /// Not named `applicationActive`, so Silica's isn't shadowed.
    property bool appActive: Qt.application.state === Qt.ApplicationActive

    /// IO stopped for no network. Only stop reason while running (costs no message).
    /// Other reasons (backgrounded) would break messages: background is their only path.
    property bool ioPaused: false

    /// Network back: start IO if stopped for lack of one; ask core.
    ///
    /// Called by connman and app open: two ways back, so a wrong watch costs a reconnect only.
    function resumeIo() {
        // Core still starting: nothing to reconsider; its IO starts fresh.
        if (core.status !== "ready") {
            return
        }
        if (appWindow.ioPaused) {
            appWindow.ioPaused = false
            core.start_all_account_io()
        }
        core.maybe_network()
    }

    /// No network for a while: stop IO once.
    ///
    /// Not gated on core: shim remembers IO asked for and resumes it on a restarted core;
    /// stopping now makes it forget, so core returns with IO stopped.
    /// Never-asked or already-stopped IO left alone.
    function pauseIo() {
        if (!appWindow.askedForIo || appWindow.ioPaused) {
            return
        }
        appWindow.ioPaused = true
        core.stop_all_account_io()
    }

    // Back in app: connection may be dead after pocket network change (IDLE takes 5 min).
    // Reconnect now.
    onAppActiveChanged: {
        if (appWindow.appActive) {
            appWindow.resumeIo()
        }
    }

    // For when nobody looks: phone announces network changes on its bus; this rescues messages.
    // See components/NetworkWatch.qml.
    NetworkWatch {
        id: networkWatch
        objectName: "networkWatch"

        onNetworkChanged: appWindow.resumeIo()

        // Gone long enough not to be a handover. Stopping gives nothing up; reconnects wake radio.
        onNetworkLost: appWindow.pauseIo()
    }

    Component.onCompleted: {
        // Drops `resumeAccountId` binding: window has answer, key is the list's.
        appWindow.resumeAccountId = appWindow.rememberedProfile()
        // Resumed phone had a profile last look: empty list means it lost it.
        appWindow.hadProfile = appWindow.resumeAccountId > 0
        // First server gets strings before any message: see DeltaChatCore.set_stock_strings.
        core.set_stock_strings(JSON.stringify(StockStrings.all()))
        core.start(rpcServerPath)
    }
}
