import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * The first screen: no address, no password -- a new Delta Chat user has
 * neither (docs/PROJECT.md), so the one thing to do here is add a
 * profile.
 *
 * Also the way back onto a chat list, for a phone the window could not
 * send there itself. The window reads `Settings.lastAccountId` before it
 * puts anything up and opens on the chat list directly when it names a
 * profile (piirit.qml), so this page is not even made on an ordinary
 * launch. What is left to it is the phone whose key was never written --
 * a profile made before the key existed, or one restored into a fresh
 * install -- where the core's own answer (`accounts_refreshed`) is the
 * only thing that knows there is a profile at all.
 *
 * Whichever of the two names a profile, the hand-over is offered until
 * the stack takes it. Silica drops an operation asked for while a
 * transition is running, and the first offer is made from inside the
 * push that puts this page up: giving up after one refusal drew the
 * whole first screen for the half second before the chat list arrived,
 * and recording the refusal as a departure left a blank screen for good.
 *
 * What it looks like: the app's own mark over its name, what it is in
 * one line, and the two ways on, in a column in the middle of an
 * otherwise empty page. Nothing behind them. There is no second chance
 * at a first impression, and a page that is only the ambience and a
 * few words reads as what it is: the start.
 *
 * Two ways on rather than one. A reader who has never heard of Delta
 * Chat is a swipe away from being told (IntroPage.qml); a reader who
 * knows what they came for goes straight to the setup path
 * (ProfileStartPage.qml). Neither is the relay dialog: picking a server
 * is a question for somebody who has already decided.
 */
Page {
    id: page

    // Both ways up: a column in the middle is a column either way.
    allowedOrientations: Orientation.All

    /// Nothing is drawn yet, because it is not yet known whether this
    /// phone has a chat list to be on. Drawing the first screen and
    /// taking it away again is the thing this page must not do.
    property bool probing: true
    /// The profile to hand over to, or 0 while none is known of. What
    /// dconf remembers, until the core says otherwise: its answer is the
    /// authority, and the only one a phone whose key was never written
    /// has.
    property int resumeAccountId: Settings.lastAccountId > 0
                                  ? Settings.lastAccountId : 0
    /// The chat list has actually been opened.
    ///
    /// Set from what the stack did, never from having asked it: a page
    /// that recorded the asking would hide itself for a hand-over that
    /// never happened and sit there blank.
    property bool leaving: false
    /// The probe has taken long enough to be worth saying so. A phone
    /// with a profile is gone from here in the time dconf takes, and a
    /// spinner that appears on the way out is the flash it was put there
    /// to prevent.
    property bool slow: false
    /// How long to go on waiting before drawing this page anyway, in
    /// milliseconds. A stack that has refused for this long is not going
    /// to take it and a core that has not answered is not going to, and
    /// a first screen the reader can use beats a blank one they cannot.
    /// Nothing sets it; a test turns it down rather than waiting.
    property int handOverDeadline: 4000

    /// Go to the chat list, if a profile to open it on is known of.
    ///
    /// IO is not asked for here: the window asks for it as soon as the
    /// core is ready, whichever page is up (piirit.qml).
    function resumeRemembered() {
        if (page.leaving || !(page.resumeAccountId > 0)) {
            return
        }
        page.leaving = page.openChatList(page.resumeAccountId)
    }

    /// Hand over to the chat list, and say whether the stack took it.
    function openChatList(accountId) {
        var opened = pageStack.replaceAbove(null,
                                            Qt.resolvedUrl("ChatListPage.qml"),
                                            { accountId: accountId })
        return opened ? true : false
    }

    // Offered until the stack takes it, every frame or so. The offer
    // costs nothing when it is refused, and one refusal says nothing
    // about the next: the stack is busy putting this very page up.
    // `triggeredOnStart` makes the first offer the moment a profile is
    // known of, which on a phone that has one is straight away.
    Timer {
        id: handOver
        objectName: "handOver"
        interval: 50
        repeat: true
        triggeredOnStart: true
        running: page.probing && !page.leaving && page.resumeAccountId > 0
        onTriggered: page.resumeRemembered()
    }

    Timer {
        id: slowProbe
        objectName: "slowProbe"
        interval: 400
        running: page.probing && !page.leaving
        onTriggered: page.slow = true
    }

    // The long stop, and the end of the offering. Whatever it is that
    // has not happened by now -- the core has not answered, the stack
    // will not take the hand-over -- waiting longer for it is worse than
    // showing the reader a screen they can do something with. It also
    // stops the timer above, so a reader who has gone on to read about
    // Delta Chat is not yanked out of it a moment later.
    Timer {
        id: handOverStop
        objectName: "handOverStop"
        interval: page.handOverDeadline
        running: page.probing && !page.leaving
        onTriggered: page.probing = false
    }

    // The core may be ready before the handler below exists.
    Component.onCompleted: {
        if (core.status === "ready") {
            core.refresh_accounts()
        } else if (core.status.indexOf("error") === 0) {
            probing = false
        }
    }

    // Qt 5.6 recognises only `onFoo:` bindings, and injects parameters
    // under the shim's snake_case names. tests/qml_syntax.rs enforces it.
    Connections {
        target: core

        onStatus_changed: {
            if (core.status === "ready") {
                core.refresh_accounts()
            } else if (core.status.indexOf("error") === 0) {
                page.probing = false
            }
        }

        // Not gated on `leaving`: a page that had really handed over is
        // gone and hears nothing, so being here to hear this means the
        // hand-over did not land, whatever it reported.
        onAccounts_refreshed: {
            if (configured_count > 0) {
                // The profile the app was closed on, which the core
                // remembers; the chat list tells it which on every open.
                // Assigned rather than handed to one attempt: the answer
                // is what the offering runs on from here, and a stack
                // that is busy this instant will not be next time.
                page.resumeAccountId = resume_account_id
                page.resumeRemembered()
            } else {
                // No profile anywhere, so there is nothing to open and
                // nothing for the next launch to open either.
                page.resumeAccountId = 0
                Settings.lastAccountId = 0
                page.probing = false
            }
        }

        onAccount_error: page.probing = false
    }

    Column {
        id: words
        anchors {
            horizontalCenter: parent.horizontalCenter
            verticalCenter: parent.verticalCenter
            // A little above the middle, where a title sits.
            verticalCenterOffset: -page.height * 0.04
        }
        // Narrower than the page, so that the line about what the app
        // is wraps as a line of reading rather than a line of screen.
        width: Math.min(parent.width - 2 * Theme.horizontalPageMargin,
                        Screen.width - 2 * Theme.horizontalPageMargin)
        spacing: Theme.paddingMedium
        // Down until it is known there is no chat list to be on. A
        // first screen drawn for the half second a hand-over takes
        // reads as the app opening in the wrong place and then
        // correcting itself.
        visible: !page.probing

        // The app's own mark over its name: the launcher icon, as it is
        // (qml/art/logo.png is icons/harbour-piirit.svg drawn out), so
        // the first screen and the launcher say the same thing. Placed
        // rather than anchored: a Column lays its children out
        // vertically and has an opinion about vertical anchors.
        Image {
            id: logo
            objectName: "logo"
            x: (parent.width - width) / 2
            width: Theme.itemSizeLarge
            height: width
            // Decoded at the size it is drawn at rather than at the
            // master's: a first screen should not cost a screenful of
            // texture for one mark.
            sourceSize.width: width
            sourceSize.height: height
            fillMode: Image.PreserveAspectFit
            smooth: true
            source: "../art/logo.png"
        }

        // The app's own name, and never a translated one (see the
        // cover): large, in the heading face, in the ambience's colour.
        Label {
            objectName: "title"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: "Piirit"
            font.family: Theme.fontFamilyHeading
            font.pixelSize: Theme.fontSizeHuge
            color: Theme.highlightColor
        }

        // One line under the name, and only one: what the app is, in
        // the words its own site uses.
        Label {
            objectName: "tagline"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: qsTr("Secure decentralised chat based on Delta Chat")
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.primaryColor
        }

        Item { width: 1; height: Theme.paddingLarge }

        // The two ways on, side by side under the name: what Delta
        // Chat is, for a reader who has not heard of it, and the way
        // into a profile for one who has -- under the mark for an
        // account, drawn by this app so that it is there on every
        // phone (components/AccountMark.qml).
        ChoiceTiles {
            objectName: "welcomeWays"
            width: parent.width
            // The column is already inside the page's margins.
            sideMargin: 0
            choices: [
                {
                    name: "about",
                    icon: "icon-m-about",
                    text: qsTr("Tell me about Delta Chat")
                },
                {
                    name: "setup",
                    mark: "account",
                    text: qsTr("Set up my profile"),
                    enabled: core.status === "ready"
                }
            ]
            onChosen: {
                if (name === "about") {
                    pageStack.push(Qt.resolvedUrl("IntroPage.qml"), {})
                } else {
                    pageStack.push(Qt.resolvedUrl("ProfileStartPage.qml"), {})
                }
            }
        }

        // The one thing that can go wrong before anything has been
        // asked for: the core did not start. Said here, where the tile
        // that it stops is.
        Label {
            objectName: "coreError"
            width: parent.width
            visible: core.status.indexOf("error") === 0
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.errorColor
            text: core.status
        }
    }

    BusyIndicator {
        objectName: "probeSpinner"
        anchors.centerIn: parent
        running: page.probing && page.slow && !page.leaving
        size: BusyIndicatorSize.Large
    }
}
