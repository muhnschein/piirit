import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * First screen: no address or password (docs/PROJECT.md), just add a profile.
 * Window skips it when `Settings.lastAccountId` names a profile (piirit.qml);
 * left: key never written (profile made before key, or restored into fresh
 * install), core's `accounts_refreshed` only sign of a profile.
 * Hand-over offered till stack takes it: Silica drops ops mid-transition, first
 * offer is inside the push putting this up. One refusal = give up flashed first
 * screen; refusal as departure left it blank.
 * Two ways on: IntroPage.qml (new users), ProfileStartPage.qml (setup). No relay
 * dialog: server picking is for the decided.
 */
Page {
    id: page

    allowedOrientations: Orientation.All

    /// Nothing drawn till chat list known.
    property bool probing: true
    /// Profile to hand over to; 0 unknown. dconf till core (authority) answers.
    property int resumeAccountId: Settings.lastAccountId > 0
                                  ? Settings.lastAccountId : 0
    /// Chat list opened: set from stack's result, never asking, else hides blank.
    property bool leaving: false
    /// Probe slow; spinner on way out is the flash avoided.
    property bool slow: false
    /// Wait before drawing anyway, ms; tests lower it.
    property int handOverDeadline: 4000

    /// Go to chat list if profile known. No IO start: window asks once core ready (piirit.qml).
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

    // Offered each frame till taken; refusal free, says nothing of next (stack busy putting
    // this up). `triggeredOnStart`: first offer once profile known.
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

    // Long stop ends offering (and timer above, so reader on IntroPage isn't yanked): waiting
    // longer is worse than usable screen.
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

    // Qt 5.6 recognises only `onFoo:` bindings, snake_case params (tests/qml_syntax.rs).
    Connections {
        target: core

        onStatus_changed: {
            if (core.status === "ready") {
                core.refresh_accounts()
            } else if (core.status.indexOf("error") === 0) {
                page.probing = false
            }
        }

        // Not gated on `leaving`: handed-over page is gone; here means failure.
        onAccounts_refreshed: {
            if (configured_count > 0) {
                // Profile app closed on (core remembers). Assigned: offering runs on it.
                page.resumeAccountId = resume_account_id
                page.resumeRemembered()
            } else {
                // No profile: nothing to open now or next launch.
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
            // Little above middle, title height.
            verticalCenterOffset: -page.height * 0.04
        }
        // Narrower: description wraps as reading line.
        width: Math.min(parent.width - 2 * Theme.horizontalPageMargin,
                        Screen.width - 2 * Theme.horizontalPageMargin)
        spacing: Theme.paddingMedium
        // Hidden till no chat list known: no flash.
        visible: !page.probing

        // Launcher icon (qml/art/logo.png = icons/harbour-piirit.svg rendered). Placed, not
        // anchored: Column ignores vertical anchors.
        Image {
            id: logo
            objectName: "logo"
            x: (parent.width - width) / 2
            width: Theme.itemSizeLarge
            height: width
            // Decode at drawn size: no screenful of texture.
            sourceSize.width: width
            sourceSize.height: height
            fillMode: Image.PreserveAspectFit
            smooth: true
            source: "../art/logo.png"
        }

        // App name, never translated (see cover).
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

        // What app is, in its site's words.
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

        // What Delta Chat is; profile entry. Own account mark: on every phone
        // (components/AccountMark.qml).
        ChoiceTiles {
            objectName: "welcomeWays"
            width: parent.width
            // Column inside page margins.
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

        // Only pre-ask failure, core didn't start: shown where its tile is.
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
