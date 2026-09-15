import QtQuick 2.0
import Sailfish.Silica 1.0
import "pages"
import "cover"
import "components"

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

    // Nothing is handled here any more: the cover's action was removed
    // along with the status label it was drawn on top of, and tapping
    // the cover already opens the app.
    cover: Component { CoverPage {} }

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

    // The two settings the core has to be told about: each applies to
    // every profile, and follows its key as the settings page changes it.
    Binding {
        target: core
        property: "download_limit"
        value: Settings.downloadLimit
    }
    Binding {
        target: core
        property: "delete_device_after"
        value: Settings.deleteDeviceAfter
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
            if (core.status === "ready" && !appWindow.askedForIo) {
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

    // Coming back to the app is the one moment the reader is watching for
    // a message, and the likeliest moment for the connection the core is
    // holding to be a dead one -- the phone has been in a pocket through
    // a change of network, and a connection killed that way says nothing
    // until the core's IDLE times out five minutes later. Asking here
    // turns that wait into a reconnection now. Nothing is asked while the
    // core is away: a core that is still starting has no connection to
    // reconsider, and the IO it starts with is a fresh one anyway.
    onAppActiveChanged: {
        if (appWindow.appActive && core.status === "ready") {
            core.maybe_network()
        }
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
