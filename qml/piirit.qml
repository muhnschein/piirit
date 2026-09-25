import QtQuick 2.0
import Sailfish.Silica 1.0
import "pages"
import "cover"
import "components"
import "js/QuickActions.js" as QuickActions

ApplicationWindow {
    id: appWindow

    /// The profile this launch opens on, or 0 on a phone that has never
    /// been on one: what the chat list wrote the last time the app was
    /// closed, read from dconf in the time it takes to open a file.
    ///
    /// Taken once, and then frozen (see `Component.onCompleted`): the
    /// key is written again while the app runs, and what it says next
    /// must not reach back into the page already on screen.
    property int resumeAccountId: appWindow.rememberedProfile()

    /// What dconf remembers, as a number. `> 0` rather than a plain
    /// read: dconf hands back `undefined` before it has read a key.
    function rememberedProfile() {
        return Settings.lastAccountId > 0 ? Settings.lastAccountId : 0
    }

    // A phone that has been used opens on its chat list. The welcome
    // page is not drawn and then replaced -- it is never made. Going
    // through it cost a page of its own: put up, asked to hand over,
    // animated out, all of that on screen before the chat list arrived.
    initialPage: appWindow.resumeAccountId > 0 ? resumedChats : firstScreen

    Component {
        id: firstScreen
        WelcomePage {}
    }
    Component {
        id: resumedChats
        ChatListPage { accountId: appWindow.resumeAccountId }
    }

    // The cover offers the quick actions the reader set up, and hands a
    // tap on one back here.
    cover: Component {
        CoverPage {
            quickActionsAllowed: appWindow.chatList !== null
                                 && !appWindow.quickActionsPaused
            onQuickAction: appWindow.quickAction(side)
        }
    }

    /// The chat list the app is on, which is where a quick action lands;
    /// null while there is none -- before the first profile, and on the
    /// way back to the welcome page after the last. The list says so
    /// itself.
    property Item chatList: null

    /// Whether a page is up that a quick action must not jump away from:
    /// a backup being written or read back, a profile being made, moved
    /// or joined from another device, a code being shown or read. Each
    /// such page says so with `pausesQuickActions`.
    readonly property bool quickActionsPaused: {
        // Read so this is worked out again whenever the stack moves.
        var depth = pageStack.depth
        for (var shown = pageStack.currentPage; shown;
             shown = pageStack.previousPage(shown)) {
            if (shown.pausesQuickActions === true) {
                return true
            }
        }
        return false
    }

    /// A quick action on the cover was tapped: up comes the app, and the
    /// chat list does the rest, the way it opens a tapped notification.
    function quickAction(side) {
        var action = QuickActions.shown(Settings, side)
        if (appWindow.chatList === null || action.kind === "") {
            return
        }
        appWindow.chatList.quickAction(action.kind, action.accountId, action.chatId)
    }

    /// The profile the app is on, as the chat list last opened it.
    ///
    /// Kept here because a share arrives at the window rather than at a
    /// page: what is shared goes into a chat of whichever profile is
    /// being read, and 0 means there is none yet -- the app is still on
    /// its way in, and there is nothing to share into.
    property int accountId: 0

    /// Something was shared to the app. Ask which chat it is for, then
    /// open that chat with it already in hand: the file on the
    /// attachment bar, the text in the field, for the reader to add to
    /// and send.
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

    /// Open the chat a share was pointed at, carrying what was shared.
    ///
    /// Replacing the picker rather than pushing over it: what the reader
    /// asked for is the chat, and the way back from it is to where they
    /// were before the share.
    function openShared(chatId, chatName, filePath, text) {
        pageStack.replace(Qt.resolvedUrl("pages/ConversationPage.qml"), {
            accountId: appWindow.accountId,
            chatId: chatId,
            chatName: chatName,
            sharedFile: filePath,
            sharedText: text
        })
    }

    // The share sheet's side of the app. Loaded rather than declared:
    // `Sailfish.Share` resolves only on a release that ships it, and an
    // import that does not resolve would otherwise take down the window
    // every page is loaded into. See qml/share/ShareTarget.qml.
    Loader {
        id: shareTarget
        objectName: "shareTarget"
        source: Qt.resolvedUrl("share/ShareTarget.qml")
    }

    // The three settings the core has to be told about: each applies to
    // every profile, and follows its key as the settings page changes it.
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
    // Whose calls ring here: nobody's once the reader has switched
    // ringing off, contacts' otherwise -- the core's own 2 and 1. A call
    // from somebody who may not ring is still kept, and can still be
    // answered from its chat.
    //
    // Only the reader's own choice writes 2. With calls off the core is
    // left at its default, 1: the key is in the profile's database and
    // goes wherever the profile does -- a backup, a second device -- and a
    // "nobody" written for a feature switched off here made a Delta Chat
    // set up from it ring for nobody, never having been asked. With calls
    // off nothing here rings anyway: CallCenter does not listen.
    Binding {
        target: core
        property: "who_can_call_me"
        value: Settings.callsEnabled === true && Settings.callsRing === false ? 2 : 1
    }

    /// The app's one call, which belongs to no page: see
    /// components/CallCenter.qml.
    CallCenter {
        id: callCenter
        objectName: "callCenter"
        // `=== true` because dconf hands back `undefined` before it has
        // read the key.
        enabled: Settings.callsEnabled === true
        stack: pageStack
        onRaise: appWindow.activate()
        // A missed call's notification, tapped: its chat, the way a
        // quick action opens one, whichever profile it is in.
        onChatRequested: {
            if (appWindow.chatList !== null) {
                appWindow.chatList.quickAction("chat", accountId, chatId)
            }
        }
    }

    /// Whether a call is under way. What keeps the call's page where it
    /// is: a tap on a notification or a quick action brings the app
    /// forward, and does not take the reader out of a call.
    readonly property bool callBusy: callCenter.busy

    /// Call a chat. False when the app is in a call already.
    function placeCall(accountId, chatId) {
        return callCenter.place(accountId, chatId)
    }

    /// Bring the call there is back to the front.
    function showCall() {
        callCenter.show()
    }

    /// Take up a call still ringing, from its row in the chat.
    function pickUpCall(accountId, chatId, messageId) {
        callCenter.pickUp(accountId, chatId, messageId)
    }

    /// IO has been asked for. Once, however long the app runs.
    property bool askedForIo: false

    /// Whether this run has ever had a profile to show.
    ///
    /// What tells the two empty account lists apart. A phone with no
    /// profile yet is already looking at the welcome page and must not
    /// be sent there again; a phone whose last profile has just gone --
    /// deleted, or the app's data cleared under it -- has a chat list
    /// open on an account the core no longer has, which is a page that
    /// cannot read or write anything and answers a tap on a chat with
    /// "account not found".
    property bool hadProfile: false

    /// The way back to the welcome page, held until the stack can make
    /// it. The last profile goes as the profiles page is leaving, so
    /// this move is asked for mid-pop -- which a stack in the middle of
    /// a transition refuses.
    PendingNavigation {
        id: welcomeAgain
        stack: pageStack
    }

    /// The way to another profile's chats, held the same way and for the
    /// same reason: the profile the chat list is on goes as the profiles
    /// page is leaving.
    PendingNavigation {
        id: otherProfile
        stack: pageStack
    }

    /// Move everything the window holds off profiles the core no longer
    /// has, onto `resumeAccountId`, the one the core says to show.
    ///
    /// Deleting the profile the chat list was on, with another left, used
    /// to leave the list open on it -- and dconf remembering it, so the
    /// next launch resumed onto it too. Every page opened from there took
    /// its profile off the list and answered with the core's "account
    /// with id N not found": the chats, the settings, the quick actions'
    /// chat picker. Asked on every refresh, so a profile remembered from
    /// before this was fixed, or deleted from another client, is left the
    /// first time the core lists the profiles there are.
    function leaveGoneProfiles(resumeAccountId) {
        // A quick action's chat went with its profile. Nothing, rather
        // than a chat the cover cannot open and the picker cannot list.
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
        // Now rather than when the new list is up: the app may be closed
        // before the move is made, and the next launch must not resume
        // onto the profile that is gone. And a refresh arriving before
        // then finds nothing left to move.
        appWindow.accountId = resumeAccountId
        Settings.lastAccountId = resumeAccountId
        otherProfile.replaceAbove(null, Qt.resolvedUrl("pages/ChatListPage.qml"),
                                  { accountId: resumeAccountId })
    }

    // Where the app goes when the last profile is gone. Here rather
    // than on the pages, which is where it used to be: the profiles
    // page is destroyed by the same swipe that deletes from it, the
    // chat list underneath it is mid-transition, and between them the
    // move was made twice or not at all. The window is neither.
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
            // Nothing to share into, and nothing to come back to.
            appWindow.accountId = 0
            Settings.lastAccountId = 0
            welcomeAgain.replaceAbove(null,
                                      Qt.resolvedUrl("pages/WelcomePage.qml"),
                                      {})
        }
    }

    // IO belongs to the window rather than to whichever page happens to
    // be up: a phone that resumes onto its chat list never sees the
    // welcome page at all. Every profile, not only the one on screen --
    // each of them is one people write to, and the cover counts them all.
    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onStatus_changed: {
            if (core.status !== "ready") {
                return
            }
            // The network went before the core was up -- a phone opened
            // in a basement -- so there is nothing to start IO for. Marked
            // as asked for and stopped instead, which is what `resumeIo`
            // reads when the network is back. Also true of a core that
            // died and came back mid-outage, which `pauseIo` has already
            // dealt with: the stop it sent an absent core is what keeps
            // this one from resuming IO, and the guard in `pauseIo` makes
            // this a no-op.
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

    /// Whether the app is the one being looked at.
    ///
    /// Held as a property rather than read where it is wanted: what
    /// matters is the moment it becomes true again, and a binding is what
    /// notices that. Silica's own `applicationActive` is not used, so
    /// nothing here shadows it.
    property bool appActive: Qt.application.state === Qt.ApplicationActive

    /// Whether IO was stopped because the phone has no network.
    ///
    /// The one reason IO is ever stopped while the app is running, and it
    /// costs no message: nothing arrives over a network that is not there.
    /// Stopping it for any other reason -- the app being backgrounded, say
    /// -- would stop the app working, because the app in the background is
    /// the only way a message reaches this platform.
    property bool ioPaused: false

    /// There is a network again, as far as anything here knows: start IO
    /// if it was stopped for want of one, and ask the core to look at what
    /// it has now.
    ///
    /// Called by everything that could mean the network is back -- connman
    /// saying so, and the reader opening the app. More than one way back is
    /// the point: a watch that got the loss wrong costs a reconnection,
    /// and a watch that got it wrong with only one way back would cost the
    /// messages.
    function resumeIo() {
        // A core that is still starting has no connection to reconsider,
        // and the IO it starts with is a fresh one anyway.
        if (core.status !== "ready") {
            return
        }
        if (appWindow.ioPaused) {
            appWindow.ioPaused = false
            core.start_all_account_io()
        }
        core.maybe_network()
    }

    /// There is no network and has not been for a while: stop IO, once.
    ///
    /// Not gated on the core being there, on purpose. A core that is away
    /// -- restarting after it died -- has no IO to stop, but the shim
    /// remembers what IO was asked for and resumes it under whatever core
    /// comes back; asking it to stop now is what makes it forget, so the
    /// core comes back with IO stopped rather than reconnecting to nothing
    /// until the next time connman changes its mind. IO that was never
    /// asked for is not the window's to stop, and IO already stopped need
    /// not be stopped again.
    function pauseIo() {
        if (!appWindow.askedForIo || appWindow.ioPaused) {
            return
        }
        appWindow.ioPaused = true
        core.stop_all_account_io()
    }

    // Coming back to the app is the one moment the reader is watching for
    // a message, and the likeliest moment for the connection the core is
    // holding to be a dead one -- the phone has been in a pocket through
    // a change of network, and a connection killed that way says nothing
    // until the core's IDLE times out five minutes later. Asking here
    // turns that wait into a reconnection now.
    onAppActiveChanged: {
        if (appWindow.appActive) {
            appWindow.resumeIo()
        }
    }

    // The other half of the ask above, and the half that matters when
    // nobody is looking: the phone announces every change of network on
    // its own bus, and a message that arrives while it is in a pocket is
    // one only this can rescue. See components/NetworkWatch.qml.
    NetworkWatch {
        id: networkWatch
        objectName: "networkWatch"

        onNetworkChanged: appWindow.resumeIo()

        // The network has been gone long enough that it is not a handover.
        // Left alone, the core would spend that time reconnecting to
        // nothing, and each attempt wakes the radio for a failure. Nothing
        // is given up by stopping.
        onNetworkLost: appWindow.pauseIo()
    }

    Component.onCompleted: {
        // Takes the binding off `resumeAccountId`: the window has its
        // answer, and from here the key belongs to the chat list.
        appWindow.resumeAccountId = appWindow.rememberedProfile()
        // A resumed phone had a profile last time it was looked at, so
        // an account list that comes back empty is one that has lost it.
        appWindow.hadProfile = appWindow.resumeAccountId > 0
        core.start(rpcServerPath)
    }
}
