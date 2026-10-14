import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * First screen: no address or password (docs/PROJECT.md), just add a profile.
 * Window opens chat list directly when `Settings.lastAccountId` names a
 * profile (piirit.qml), so this page isn't made on ordinary launch. Left to
 * it: phones whose key was never written (profile made before key, or
 * restored into fresh install), where core's `accounts_refreshed` is the only
 * sign of a profile.
 * Hand-over offered until stack takes it: Silica drops ops during transitions,
 * and first offer is made inside the push putting this page up. Giving up
 * after one refusal flashed first screen; recording refusal as departure left
 * it blank for good.
 * Look: app mark, name, one line, two ways on (IntroPage.qml for new users,
 * ProfileStartPage.qml for setup). No relay dialog: picking a server is for
 * those already decided.
 */
Page {
    id: page

    // Both ways up, in a column.
    allowedOrientations: Orientation.All

    /// Nothing drawn until known whether phone has a chat list.
    property bool probing: true
    /// Profile to hand over to, or 0 if unknown. dconf until core answers; core is authority.
    property int resumeAccountId: Settings.lastAccountId > 0
                                  ? Settings.lastAccountId : 0
    /// Chat list actually opened. Set from stack's result, never from asking:
    /// else page hides for a hand-over that never happened, blank.
    property bool leaving: false
    /// Probe is slow. Spinner on the way out is the flash it prevents.
    property bool slow: false
    /// Wait before drawing anyway, ms. Tests turn it down, never wait.
    property int handOverDeadline: 4000

    /// Go to chat list if a profile to open on is known.
    ///
    /// No IO start here: window asks once core is ready (piirit.qml).
    function resumeRemembered() {
        if (page.leaving || !(page.resumeAccountId > 0)) {
            return
        }
        page.leaving = page.openChatList(page.resumeAccountId)
    }

    /// Hand over to chat list; returns whether stack took it.
    function openChatList(accountId) {
        var opened = pageStack.replaceAbove(null,
                                            Qt.resolvedUrl("ChatListPage.qml"),
                                            { accountId: accountId })
        return opened ? true : false
    }

    // Offered every frame until taken. Refusal is free and says nothing about
    // the next (stack busy putting this page up).
    // `triggeredOnStart`: first offer as soon as a profile is known.
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

    // Long stop: ends offering. Waiting longer is worse than a usable screen.
    // Also stops the timer above, so reader gone on to IntroPage isn't yanked
    // away.
    Timer {
        id: handOverStop
        objectName: "handOverStop"
        interval: page.handOverDeadline
        running: page.probing && !page.leaving
        onTriggered: page.probing = false
    }

    // Core may be ready before the handler below exists.
    Component.onCompleted: {
        if (core.status === "ready") {
            core.refresh_accounts()
        } else if (core.status.indexOf("error") === 0) {
            probing = false
        }
    }

    // Qt 5.6 only recognises `onFoo:` bindings, with snake_case params. Enforced
    // by tests/qml_syntax.rs.
    Connections {
        target: core

        onStatus_changed: {
            if (core.status === "ready") {
                core.refresh_accounts()
            } else if (core.status.indexOf("error") === 0) {
                page.probing = false
            }
        }

        // Not gated on `leaving`: a page that handed over is gone, so being here means failure.
        onAccounts_refreshed: {
            if (configured_count > 0) {
                // Profile app closed on (core remembers). Assigned, not one
                // attempt: offering runs on it; busy stack may take it later.
                page.resumeAccountId = resume_account_id
                page.resumeRemembered()
            } else {
                // No profile anywhere: nothing to open, nothing for next launch either.
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
            // Little above middle, where a title sits.
            verticalCenterOffset: -page.height * 0.04
        }
        // Narrower than page: description wraps as reading line.
        width: Math.min(parent.width - 2 * Theme.horizontalPageMargin,
                        Screen.width - 2 * Theme.horizontalPageMargin)
        spacing: Theme.paddingMedium
        // Down until known there is no chat list. Avoids a flash in the wrong place.
        visible: !page.probing

        // App mark over name: launcher icon (qml/art/logo.png is
        // icons/harbour-piirit.svg rendered), so first screen and launcher
        // match.
        // Placed, not anchored: Column ignores vertical anchors.
        Image {
            id: logo
            objectName: "logo"
            x: (parent.width - width) / 2
            width: Theme.itemSizeLarge
            height: width
            // Decoded at drawn size: first screen should not cost a screenful of texture.
            sourceSize.width: width
            sourceSize.height: height
            fillMode: Image.PreserveAspectFit
            smooth: true
            source: "../art/logo.png"
        }

        // App name, never translated (see cover): large heading face.
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

        // One line under name: what app is, in its own site's words.
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

        // Two ways on under name: what Delta Chat is, and profile entry.
        // Account mark drawn by this app so it's on every phone (components/AccountMark.qml).
        ChoiceTiles {
            objectName: "welcomeWays"
            width: parent.width
            // Column already inside page margins.
            sideMargin: 0
            choices: [
                {
                    name: "about",
                    icon: "icon-m-about",
                    text: qsTr("About Delta Chat")
                },
                {
                    name: "setup",
                    mark: "account",
                    text: qsTr("Set up your profile"),
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

        // The one pre-ask failure: core didn't start. Said where the tile it stops is.
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
