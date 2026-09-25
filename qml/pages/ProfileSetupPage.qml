import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * The work behind AddProfileDialog: the core asks the relay for an
 * account, stores what it answers, and starts IO; this page shows the
 * progress meanwhile and lands in the chat list when it is done. Cancel
 * and a failure both go back to the dialog, which still has what was
 * typed.
 *
 * Silica makes a dialog's accept destination the moment the dialog comes
 * on screen, so that it can be peeked at, and the dialog fills the name
 * and relay in on accept. So nothing happens here on creation: the work
 * starts when the page is the one on screen, which is after both.
 *
 * A relay is somebody's spare-time server, and one that is down holds
 * the core for minutes. So the page says as much once the wait has gone
 * past four seconds, under the Cancel button that has been there all
 * along; and the shim gives up on the relay for the reader at thirty
 * (signup.rs), which lands here as a time-out with the way back.
 */
Page {
    id: page

    /// The cover offers no quick actions while this is up: a jump away
    /// would leave a profile halfway made. See piirit.qml.
    readonly property bool pausesQuickActions: true

    property string displayName
    property string providerQr
    /// The relay's name, for the page to say.
    readonly property string relay: page.providerQr.replace("dcaccount:", "")

    // True while the core is working.
    property bool busy: false
    // Set once the core has been asked, so a second activation -- coming
    // back from a page pushed on top -- does not ask again.
    property bool started: false
    property int permille: 0
    property string errorMessage: ""
    // Set once the relay has taken longer than four seconds, and kept:
    // the reason to try another relay does not go away with the error
    // that may follow.
    property bool slowRelay: false

    // The four seconds. Counted only while the core is working, and
    // from the start of it: the page is made before it is on screen.
    Timer {
        id: patience
        interval: 4000
        running: page.busy
        onTriggered: page.slowRelay = true
    }

    // Cancel is the way back while the core is working: a swipe back
    // would drop this page, and with it the handler that opens the chat
    // list, leaving a profile made and nothing shown for it.
    backNavigation: !busy

    // A move the core asked for, held until this page is the one on
    // screen: a call's page may be over it, and a move made then would
    // take that page instead (PendingNavigation.qml).
    PendingNavigation {
        id: navigation
        stack: pageStack
        ready: page.status === PageStatus.Active
    }

    function begin() {
        if (page.started) {
            return
        }
        page.started = true
        page.busy = true
        page.permille = 0
        page.slowRelay = false
        core.create_profile(displayName, providerQr)
    }

    onStatusChanged: {
        if (page.status === PageStatus.Active) {
            page.begin()
        }
    }
    // Pushed straight to the top, the page is active before the handler
    // above exists.
    Component.onCompleted: {
        if (page.status === PageStatus.Active) {
            page.begin()
        }
    }

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core

        onProfile_created: {
            // Only while this page is waiting: an answer after cancel is
            // not this page's to act on.
            if (!page.busy) {
                return
            }
            page.busy = false
            navigation.replaceAbove(null, Qt.resolvedUrl("ChatListPage.qml"),
                                    { accountId: account_id })
        }

        onProfile_error: {
            if (!page.busy) {
                return
            }
            page.busy = false
            page.errorMessage = message
        }

        // The shim gave up on the relay. Worded here rather than in the
        // shim: it is the one failure the app decides on itself, so it
        // is the one the app can say in the reader's language.
        onProfile_timed_out: {
            if (!page.busy) {
                return
            }
            page.busy = false
            page.errorMessage = qsTr("%1 did not answer within %2 seconds.")
                                .arg(page.relay).arg(seconds)
        }

        // Not gated on `busy`: the core's last progress events can arrive
        // after the call that started them has already been answered.
        onConfigure_progress: page.permille = permille
    }

    PageHeader {
        id: header
        title: qsTr("Add profile")
    }

    Column {
        anchors.centerIn: parent
        width: page.width
        spacing: Theme.paddingLarge

        ProgressBar {
            objectName: "progressBar"
            width: parent.width
            visible: page.busy
            minimumValue: 0
            maximumValue: 1000
            value: page.permille
            label: qsTr("Contacting %1…").arg(page.relay)
        }

        Button {
            objectName: "cancelButton"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: page.busy
            text: qsTr("Cancel")
            onClicked: {
                page.busy = false
                core.cancel_ongoing()
                pageStack.pop()
            }
        }

        // Under the Cancel button, once the relay has kept the reader
        // waiting: what a relay is, and what to do about one that does
        // not answer. Stays through the error that may follow, since
        // the Back button under it is the way it points.
        Label {
            objectName: "slowHint"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            visible: page.slowRelay
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.secondaryHighlightColor
            text: qsTr("Could not reach the relay. Try another one.")
        }

        Banner {
            objectName: "errorBanner"
            width: parent.width
            text: page.errorMessage
            onDismissed: page.errorMessage = ""
        }

        Button {
            objectName: "backButton"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: !page.busy
            text: qsTr("Back")
            onClicked: pageStack.pop()
        }
    }
}
