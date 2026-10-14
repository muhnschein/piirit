import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Relays.js" as Relays

/*
 * Extra relay for profile, from profile page (ProfilePage.qml).
 *
 * Same call that made profile (add_transport_from_qr, docs/PROJECT.md): relay
 * half of AddProfileDialog, then ProfileSetupPage's wait, on one page.
 *
 * Relay = spare-time server: one down holds core minutes. Past 4 s page says
 * so; shim gives up at 30 s (signup.rs).
 *
 * Done: page goes; relay on profile, page below re-reads relays.
 *
 * Scanned QR (`scannedQr`) lands here, relay chosen; added on button press.
 */
Page {
    id: page

    /// No quick actions: jump away would leave relay half-added. See piirit.qml.
    readonly property bool pausesQuickActions: true

    property int accountId

    /// Scanned code (`dcaccount:`/`dclogin:`), passed as is; empty if picked.
    property string scannedQr: ""
    /// Relay the code names, as core read it.
    property string scannedRelay: ""
    readonly property bool scanned: page.scannedQr.length > 0

    /// Relay chosen: scanned, else typed, else picked.
    property string domain: page.scanned ? page.scannedRelay
                            : customField.text.trim().length > 0
                            ? customField.text.trim()
                            : (relayCombo.currentIndex >= 0 && relayCombo.currentIndex < relays.length
                               ? relays[relayCombo.currentIndex].domain : "")
    /// Core gets `dcaccount:` + relay. Empty until chosen.
    property string providerQr: page.scanned ? page.scannedQr
                                : (domain.length > 0 ? "dcaccount:" + domain : "")

    // Relays.js (chatmail.at/relays).
    readonly property var relays: Relays.list

    /// True while core works.
    property bool busy: false
    property int permille: 0
    property string errorMessage: ""
    /// Set after 4 s of relay wait; kept through the error after it.
    property bool slowRelay: false

    // 4 s, counted only while core works.
    Timer {
        id: patience
        interval: 4000
        running: page.busy
        onTriggered: page.slowRelay = true
    }

    // Cancel is the way back while busy: swipe would drop page and its answer
    // handler, leaving core on relay with nobody listening.
    backNavigation: !page.busy

    // Core move held until page on screen: call page may cover it, and move
    // then would take that page instead.
    PendingNavigation {
        id: navigation
        stack: pageStack
        ready: page.status === PageStatus.Active
    }

    function begin() {
        if (page.busy || page.providerQr.length === 0) {
            return
        }
        page.errorMessage = ""
        page.permille = 0
        page.slowRelay = false
        page.busy = true
        core.add_relay(page.accountId, page.providerQr)
    }

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core

        // Only while waiting: answer after Cancel is not ours.
        onRelay_added: {
            if (!page.busy || account_id !== page.accountId) {
                return
            }
            page.busy = false
            navigation.pop()
        }

        onRelay_error: {
            if (!page.busy) {
                return
            }
            page.busy = false
            page.errorMessage = message
        }

        // Shim gave up on relay: app's own failure, so worded here.
        onRelay_timed_out: {
            if (!page.busy) {
                return
            }
            page.busy = false
            page.errorMessage = qsTr("%1 did not answer within %2 seconds.")
                                .arg(page.domain).arg(seconds)
        }

        // Not gated on `busy`: late progress can land after call answered.
        // Gated on account: same signal reports every profile.
        onConfigure_progress: {
            if (account_id === page.accountId) {
                page.permille = permille
            }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: page.width
            spacing: Theme.paddingLarge

            PageHeader {
                title: qsTr("Add a relay")
            }

            Label {
                objectName: "intro"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: page.scanned
                      //: Delta Chat's question, over a relay read off a scanned code.
                      ? qsTr("Add this relay?")
                      : qsTr("Adds another address for this profile on a second relay.")
            }

            // The relay the scanned code names, in place of the list.
            Label {
                objectName: "scannedRelay"
                visible: page.scanned
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.WrapAnywhere
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeLarge
                // The code's own string, pinned to plain.
                textFormat: Text.PlainText
                text: page.scannedRelay
            }

            ComboBox {
                id: relayCombo
                objectName: "relayCombo"
                visible: !page.scanned
                width: parent.width
                // Said as the action it is, as add-profile dialog says.
                label: qsTr("Select a public chatmail relay")
                // Nothing to start: reader picks, as for profile itself.
                currentIndex: -1
                // Off when server typed (it wins) or while core busy.
                enabled: !page.busy && customField.text.trim().length === 0

                menu: ContextMenu {
                    Repeater {
                        model: page.relays

                        MenuItem {
                            objectName: "relayOption" + index
                            text: Relays.label(modelData)
                        }
                    }
                }
            }

            TextField {
                id: customField
                objectName: "customField"
                visible: !page.scanned
                width: parent.width
                label: qsTr("Use a custom chatmail relay")
                placeholderText: label
                readOnly: page.busy
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhUrlCharactersOnly
            }

            Label {
                objectName: "relaysHint"
                visible: !page.scanned
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                linkColor: Theme.highlightColor
                textFormat: Text.StyledText
                text: qsTr("Piirit works only with chatmail relays. These are a particular kind of e-mail server; ordinary e-mail servers are not supported. For more, see <a href=\"https://chatmail.at\">chatmail.at</a>. A full list of public, free-to-use chatmail relays is at <a href=\"https://chatmail.at/relays\">chatmail.at/relays</a>.")
                onLinkActivated: Qt.openUrlExternally(link)
            }

            // Dim until relay chosen, as dialog's Create.
            Button {
                objectName: "addButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !page.busy
                enabled: page.domain.length > 0
                text: qsTr("Add relay")
                onClicked: page.begin()
            }

            ProgressBar {
                objectName: "progressBar"
                width: parent.width
                visible: page.busy
                minimumValue: 0
                maximumValue: 1000
                value: page.permille
                label: qsTr("Contacting %1…").arg(page.domain)
            }

            Button {
                objectName: "cancelButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.busy
                text: qsTr("Cancel")
                onClicked: {
                    page.busy = false
                    core.cancel_ongoing()
                }
            }

            // Under Cancel once `slowRelay`: what a relay is, what to do if it
            // does not answer. Stays through following error.
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
        }
    }
}
