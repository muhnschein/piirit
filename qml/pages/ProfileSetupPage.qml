import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Work behind AddProfileDialog: core gets account from relay, stores it,
 * starts IO; page shows progress, then lands in chat list. Cancel or failure:
 * back to dialog, which keeps input.
 * Silica makes accept destination when dialog shows; dialog fills name and
 * relay on accept. So work starts once on screen, not at creation.
 * Relay past 4 s: say so under Cancel. Shim gives up at 30 s (signup.rs).
 */
Page {
    id: page

    /// No quick actions: jump away leaves profile half made. See piirit.qml.
    readonly property bool pausesQuickActions: true

    property string displayName
    property string providerQr
    /// Relay name to show. Empty when core picks relays (AddProfileDialog.qml).
    readonly property string relay: page.providerQr.replace("dcaccount:", "")

    // True while core works.
    property bool busy: false
    // Set once core asked: re-activation must not re-ask.
    property bool started: false
    property int permille: 0
    property string errorMessage: ""
    // Set once relay past 4 s; outlives the error.
    property bool slowRelay: false

    // The 4 s, counted only while core works.
    Timer {
        id: patience
        interval: 4000
        running: page.busy
        onTriggered: page.slowRelay = true
    }

    // Cancel is way back while core works: swipe-back would drop handler that
    // opens chat list, leaving profile made, nothing shown.
    backNavigation: !busy

    // Core move held until on screen: call page may cover it, and move then
    // would take that page (PendingNavigation.qml).
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
    // Pushed to top: active before handler above exists.
    Component.onCompleted: {
        if (page.status === PageStatus.Active) {
            page.begin()
        }
    }

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core

        onProfile_created: {
            // Only while waiting: answer after cancel is not ours.
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

        // Shim gave up on relay. Worded here: the one failure app decides
        // itself, so it can say it in reader's language.
        onProfile_timed_out: {
            if (!page.busy) {
                return
            }
            page.busy = false
            page.errorMessage = page.relay.length > 0
                    ? qsTr("%1 did not answer within %2 seconds.")
                      .arg(page.relay).arg(seconds)
                    : qsTr("No relay answered within %1 seconds.").arg(seconds)
        }

        // Not gated on `busy`: late progress events arrive after the call.
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
            label: page.relay.length > 0 ? qsTr("Contacting %1…").arg(page.relay)
                                         : qsTr("Connecting…")
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

        // Under Cancel once relay past 4 s: what relay is, what if silent.
        // Stays through error (Back below is where it points). Not when core
        // picks relays: trying others is what it does.
        Label {
            objectName: "slowHint"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            visible: page.slowRelay && page.relay.length > 0
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
