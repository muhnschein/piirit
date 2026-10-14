import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * This phone offers one of its profiles to another device: far end of
 * RestoreProfilePage's `from: "device"`. Core transfer, both ways.
 *
 * Code behind a button: nothing tells a second device from a camera over the
 * shoulder, and code gives whole profile (messages, contacts, key), so reader
 * says when it goes up; line above says what. Not started on entry: page
 * opened by mistake shows nothing. Button: "Show code", then "Show code
 * again". Cancel takes code down, keeps reader on button.
 *
 * Leaving ends offer: a left provider holds profile out, IO paused. Only a
 * running transfer pins page.
 *
 * Profile stops fetching mail while offer up (core pauses IO). Code is
 * shim-written Image, not Canvas (see QrPage.qml).
 */
Page {
    id: page

    /// No quick actions, tapped notification waits, while offer up: jump away
    /// leaves other device half-copied. See piirit.qml. Not once over.
    readonly property bool pausesQuickActions: device.running

    /// Profile offered: core provider's account.
    property int accountId

    /// Chat list's profile: forward swipe goes here after a device takes this
    /// one. Not [[accountId]]: any profile offers without leaving its chats.
    property int currentAccountId: 0

    property string errorMessage: ""
    /// A device read the code and has the profile.
    property bool taken: false
    /// Chats attached right: a swipe to offer.
    property bool chatsAttached: false
    /// Code shown once. A start that never reached a code does not count.
    property bool shown: false

    /// Core preparing provider; nothing to show.
    readonly property bool preparing: device.running && device.code.length === 0
    /// Code up; nobody has read it.
    readonly property bool showing: device.running && device.code.length > 0
                                    && device.permille === 0
    /// Device reading profile off this one.
    readonly property bool transferring: device.running && device.permille > 0

    // Code always walkable away from; running transfer not (half-copied
    // device). Cancel is the way out.
    backNavigation: !page.transferring

    // Top row: profile name, picture, address, as profiles page draws.
    Profile {
        id: profile
        objectName: "profile"
        account_id: page.accountId
    }

    SecondDevice {
        id: device
        objectName: "device"
        account_id: page.accountId
        onTaken: {
            page.errorMessage = ""
            page.taken = true
            page.offerTheChats()
        }
        // Provider ended, nothing went over: core has no words. As restore's
        // "stalled": both ends give up on same silence.
        onStalled: page.errorMessage = qsTr("No device copied the profile. Both devices must stay on the same network with this page open.")
        onError: page.errorMessage = message
        // Button text depends on a code ever shown, not offer started.
        onCode_changed: {
            if (device.code.length > 0) {
                page.shown = true
            }
        }
    }

    // Core reports transfer like import/export, without saying which; listens
    // only while asked. See second_device.rs.
    Connections {
        target: core
        onCore_event: device.handle_event(context_id, kind, payload_json)
    }

    // Code as picture, for other phone's camera.
    QrCode {
        id: qr
        objectName: "qr"
        text: device.code
    }

    // Ended on any page leave: left provider holds profile out unseen. Cancel
    // stops without leaving. Covered (call page pushed over) is not leaving.
    Component.onDestruction: device.cancel()

    /// Start offer, on reader's word. No core check: absent core answers "not
    /// started" on `error`, a banner; better than a silent button.
    function begin() {
        if (device.running) {
            return
        }
        page.errorMessage = ""
        page.taken = false
        device.offer()
    }

    /// Chats right of this page: swipe leaves it. Attached, not pushed.
    function offerTheChats() {
        if (page.currentAccountId <= 0 || page.chatsAttached) {
            return
        }
        pageStack.pushAttached(Qt.resolvedUrl("ChatListPage.qml"),
                               { accountId: page.currentAccountId, attached: true })
        page.chatsAttached = true
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingLarge

            PageHeader {
                objectName: "header"
                title: qsTr("Add a second device")
            }

            // Profile other device gets, as profiles page draws it.
            ContactRow {
                objectName: "profileRow"
                width: parent.width
                displayName: profile.display_name.length > 0
                             ? profile.display_name : profile.address
                address: profile.address
                showAddress: true
                ownColor: profile.color
                picturePath: profile.avatar_path
                isKeyContact: true
            }

            // What to do on other device: only thing reader cannot work out
            // here.
            Label {
                objectName: "instructions"
                visible: !page.taken
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("On the other device, choose \"Add as second device\" and scan this code. Both devices must be on the same network.")
            }

            // What button puts on screen, said before press: last chance to
            // check who is in the room.
            Label {
                objectName: "caution"
                visible: !page.taken
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: qsTr("Make sure no unwanted observer or camera can see this code. This profile does not receive messages while the code is shown.")
            }

            // No code to draw or report while core starts provider.
            BusyIndicator {
                objectName: "preparing"
                anchors.horizontalCenter: parent.horizontalCenter
                running: page.preparing
                visible: page.preparing
                size: BusyIndicatorSize.Medium
            }

            // White whatever ambience, not smoothed: see QrPage.qml.
            Image {
                objectName: "deviceQr"
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.floor(parent.width * 0.7)
                height: width
                visible: page.showing && qr.size > 0
                source: qr.image
                smooth: false
                cache: false
                fillMode: Image.PreserveAspectFit
            }

            // Same string as code, for a camera that will not read it:
            // RestoreProfilePage takes it typed or pasted.
            Label {
                objectName: "codeLabel"
                visible: page.showing
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.WrapAnywhere
                // Core's own payload, pinned plain.
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: device.code
            }

            Button {
                objectName: "copyButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.showing
                text: qsTr("Copy the code")
                onClicked: Clipboard.text = device.code
            }

            // Device reading profile: transfer shown, code no longer subject.
            ProgressBar {
                objectName: "transferProgress"
                width: parent.width
                visible: page.transferring
                minimumValue: 0
                maximumValue: 1000
                value: device.permille
                label: qsTr("Transferring…")
            }

            // Stops offer, stays on page: Cancel. Button then reads "Show code
            // again". Leaving page also ends offer.
            Button {
                objectName: "cancelButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: device.running
                text: qsTr("Cancel")
                onClicked: device.cancel()
            }

            // Only action on its own: code up, again later. Gone while offer
            // runs or once device has profile.
            Button {
                objectName: "showButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !device.running && !page.taken
                text: page.shown ? qsTr("Show code again") : qsTr("Show code")
                onClicked: page.begin()
            }

            // Done: other device has profile too.
            Label {
                objectName: "takenLabel"
                visible: page.taken
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                color: Theme.highlightColor
                text: qsTr("The profile was transferred to your second device. Both devices now receive all new messages.")
            }

            // Way on, in words and indicator: only when a page exists right.
            Label {
                objectName: "onwardHint"
                visible: page.taken && page.chatsAttached
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Swipe on for your chats.")
            }
        }
    }

    // Errors: core refused offer, or code failed. Retry stays: a strip, not a
    // page.
    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }
}
