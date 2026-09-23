import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Relays.js" as Relays

/*
 * One more relay for a profile that has one. Reached from the relays on
 * the profile page (ProfilePage.qml).
 *
 * The core lets a profile be reached through several relays at once, each
 * with an address of its own; adding one is the same call that made the
 * profile in the first place, on the account the profile already is
 * (add_transport_from_qr, docs/PROJECT.md). So this page is the relay
 * half of AddProfileDialog -- the same list, the same custom field -- and
 * then the wait ProfileSetupPage shows, on one page: there is no name to
 * ask for, and nowhere to hand over to, since the profile page under this
 * one is where the relay shows up.
 *
 * A relay is somebody's spare-time server, and one that is down holds
 * the core for minutes. So the page says as much once the wait has gone
 * past four seconds, under the Cancel button that has been there all
 * along; and the shim gives up on the relay for the reader at thirty
 * (signup.rs), which lands here as a time-out with the fields back.
 *
 * Done means gone: the relay is on the profile, and its row is on the
 * page under this one, which re-reads its relays as this page goes.
 */
Page {
    id: page

    /// The cover offers no quick actions while this is up: a jump away
    /// would leave a relay halfway added. See piirit.qml.
    readonly property bool pausesQuickActions: true

    property int accountId

    /// The relay chosen: the custom one if typed, else the picked one.
    property string domain: customField.text.trim().length > 0
                            ? customField.text.trim()
                            : (relayCombo.currentIndex >= 0 && relayCombo.currentIndex < relays.length
                               ? relays[relayCombo.currentIndex].domain : "")
    /// What the core is handed: `dcaccount:` and a relay, which it takes
    /// with or without the `https://.../new` around it. Empty until a
    /// relay is chosen.
    property string providerQr: domain.length > 0 ? "dcaccount:" + domain : ""

    // The public relays, as chatmail.at/relays lists them (Relays.js).
    readonly property var relays: Relays.list

    /// True while the core is working.
    property bool busy: false
    property int permille: 0
    property string errorMessage: ""
    /// Set once the relay has taken longer than four seconds, and kept:
    /// the reason to try another relay does not go away with the error
    /// that may follow.
    property bool slowRelay: false

    // The four seconds. Counted only while the core is working.
    Timer {
        id: patience
        interval: 4000
        running: page.busy
        onTriggered: page.slowRelay = true
    }

    // Cancel is the way back while the core is working: a swipe back
    // would drop this page, and with it the handler that hears the
    // answer, leaving the core on the relay with nobody waiting.
    backNavigation: !page.busy

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

        // Only while this page is waiting: an answer after Cancel is not
        // this page's to act on, and the shim signals nothing for one.
        onRelay_added: {
            if (!page.busy || account_id !== page.accountId) {
                return
            }
            page.busy = false
            pageStack.pop()
        }

        onRelay_error: {
            if (!page.busy) {
                return
            }
            page.busy = false
            page.errorMessage = message
        }

        // The shim gave up on the relay. Worded here rather than in the
        // shim: it is the one failure the app decides on itself, so it
        // is the one the app can say in the reader's language.
        onRelay_timed_out: {
            if (!page.busy) {
                return
            }
            page.busy = false
            page.errorMessage = qsTr("%1 did not answer within %2 seconds.")
                                .arg(page.domain).arg(seconds)
        }

        // Not gated on `busy`: the core's last progress events can arrive
        // after the call that started them has already been answered.
        // Gated on the account: the same signal reports every profile.
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
                text: qsTr("Adds another address for this profile on a second relay.")
            }

            ComboBox {
                id: relayCombo
                objectName: "relayCombo"
                width: parent.width
                // Said as the action it is, as the add-profile dialog
                // says it.
                label: qsTr("Select a public chatmail relay")
                // Nothing to begin with: the reader picks, as they did
                // for the profile itself.
                currentIndex: -1
                // A typed server is the one that counts, and nothing
                // changes while the core is on the one that was chosen.
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
                width: parent.width
                label: qsTr("Use a custom chatmail relay")
                placeholderText: label
                readOnly: page.busy
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhUrlCharactersOnly
            }

            Label {
                objectName: "relaysHint"
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

            // Dim until a relay is chosen, as the dialog's Create is: a
            // relay is where an address and a mailbox live for as long
            // as the profile keeps it.
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

            // Under the Cancel button, once the relay has kept the reader
            // waiting: what a relay is, and what to do about one that does
            // not answer. Stays through the error that may follow.
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
