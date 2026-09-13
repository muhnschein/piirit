import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * The first screen: no address, no password -- a new Delta Chat user has
 * neither (docs/PROJECT.md), so the one thing to do here is add a
 * profile. Also the resume path: with a configured account it hands
 * straight over to the chat list, on the profile the app was last
 * closed on.
 *
 * That hand-over happens twice over, and the quick way is the one that
 * usually wins. `Settings.lastAccountId` is dconf, read in the time it
 * takes to open a file, so a phone that has a profile is on its chat
 * list before the core has finished starting. The core's own answer
 * (`accounts_refreshed`) is the authority and arrives later: it is what
 * a phone with no profile waits for, and what corrects a remembered
 * profile that has since been deleted. Whichever comes first wins, and
 * `leaving` stops the other one.
 *
 * What it looks like is what the cover looks like once there are
 * people: a field of faces in the ambience's colours, a few of them lit,
 * filling the screen either way up -- and in the middle, where the
 * field clears for them, the app's name, what it is, and the way on.
 * There is no second chance at a first impression, so the field is a
 * picture (components/FaceField.qml): one texture, one pass, drawn the
 * frame the page is.
 *
 * Two ways on rather than one. A reader who has never heard of Delta
 * Chat is a swipe away from being told (IntroPage.qml); a reader who
 * knows what they came for goes straight to the setup path
 * (ProfileStartPage.qml). Neither is the relay dialog: picking a server
 * is a question for somebody who has already decided.
 */
Page {
    id: page

    // Both ways up: the field has a master for each.
    allowedOrientations: Orientation.All

    // Hides the page until we know whether an account exists.
    property bool probing: true
    /// The page is on its way out. Two answers can send it there.
    property bool leaving: false
    /// The probe has taken long enough to be worth saying so. A phone
    /// with a profile is gone from here in the time dconf takes, and a
    /// spinner that appears on the way out is the flash it was put there
    /// to prevent.
    property bool slow: false

    /// Go to the chat list if this phone remembers being on one.
    ///
    /// IO is asked for here rather than left to the chat list: the core
    /// may not have started yet, and the shim keeps the request until it
    /// has. Every profile, not only the one shown -- each of them is one
    /// people write to, and the cover counts them all.
    function resumeRemembered() {
        if (page.leaving || !(Settings.lastAccountId > 0)) {
            return
        }
        page.leaving = true
        core.start_all_account_io()
        pageStack.replaceAbove(null, Qt.resolvedUrl("ChatListPage.qml"),
                               { accountId: Settings.lastAccountId })
    }

    Timer {
        id: slowProbe
        objectName: "slowProbe"
        interval: 400
        running: page.probing && !page.leaving
        onTriggered: page.slow = true
    }

    // dconf answers a moment after the page is up, so the shortcut is
    // taken from here as well as on completion.
    Connections {
        target: Settings
        onLastAccountIdChanged: page.resumeRemembered()
    }

    // The core may be ready before the handler below exists.
    Component.onCompleted: {
        page.resumeRemembered()
        if (page.leaving) {
            return
        }
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

        onAccounts_refreshed: {
            if (page.leaving) {
                return
            }
            if (configured_count > 0) {
                page.leaving = true
                // Every profile, not only the one shown: each of them is
                // one people write to, and the cover counts them all.
                core.start_all_account_io()
                // The profile the app was closed on, which the core
                // remembers; the chat list tells it which on every open.
                pageStack.replaceAbove(null, Qt.resolvedUrl("ChatListPage.qml"),
                                       { accountId: resume_account_id })
            } else {
                page.probing = false
            }
        }

        onAccount_error: page.probing = false
    }

    // The field, under everything, cleared around the words by as much
    // as they take up: the box follows the column, so a language in
    // which the line about relays runs long clears more.
    FaceField {
        id: field
        objectName: "faceField"
        anchors.fill: parent
        // Down until the core has answered. A phone with profiles on it
        // is going straight to the chat list, and a screenful of faces
        // drawn for the half second that takes reads as the app opening
        // in the wrong place and then correcting itself.
        visible: !page.probing
        source: page.width > page.height ? "../art/faces-landscape.png"
                                         : "../art/faces-portrait.png"
        // Fainter than the component's own default. The field is the
        // welcome, not the reading matter, and what it is painted from
        // carries more ink than the flat masks it started as: both
        // channels scale the colour and its alpha together, so less ink
        // is more of the ambience showing through.
        ink: 0.45
        litInk: 0.8
        clearX: words.x + words.width / 2
        clearY: words.y + words.height / 2
        clearWidth: words.width
        clearHeight: words.height
        // Room around the words rather than up against them: the field
        // is cleared this far out from the column before it begins to
        // fade back in.
        clearRadius: Theme.itemSizeExtraSmall
        clearFeather: Theme.itemSizeLarge
    }

    Column {
        id: words
        anchors {
            horizontalCenter: parent.horizontalCenter
            verticalCenter: parent.verticalCenter
            // A little above the middle, where a title sits.
            verticalCenterOffset: -page.height * 0.04
        }
        // Narrower than the page: the field is what fills it, and the
        // words are what is read.
        width: Math.min(parent.width - 2 * Theme.horizontalPageMargin,
                        Screen.width - 2 * Theme.horizontalPageMargin)
        spacing: Theme.paddingMedium
        visible: !page.probing

        // The app's own name, and never a translated one (see the
        // cover): large, in the heading face, in the ambience's colour.
        Label {
            objectName: "title"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            textFormat: Text.PlainText
            text: "Postivene"
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

        Button {
            objectName: "aboutButton"
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("Tell me about Delta Chat")
            onClicked: pageStack.push(Qt.resolvedUrl("IntroPage.qml"), {})
        }

        Button {
            objectName: "setupButton"
            anchors.horizontalCenter: parent.horizontalCenter
            text: qsTr("Set up my profile")
            enabled: core.status === "ready"
            onClicked: pageStack.push(Qt.resolvedUrl("ProfileStartPage.qml"), {})
        }

        // The one thing that can go wrong before anything has been
        // asked for: the core did not start. Said here, where the
        // buttons that it stops are.
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
