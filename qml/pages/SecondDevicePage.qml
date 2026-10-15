import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Phone offers a profile to another device: far end of RestoreProfilePage's
 * `from: "device"`. Core transfer, both ways.
 *
 * Code behind a button: nothing tells second device from camera over shoulder,
 * and code gives whole profile (messages, contacts, key), so reader says when;
 * line above says what. Not started on entry (page opened by mistake shows
 * nothing). "Show code", then "Show code again"; Cancel takes code down.
 *
 * No mail while offer up (core pauses IO); leaving ends offer, else left provider
 * holds profile out. Only running transfer pins page. Code: shim-written Image,
 * not Canvas (see QrPage.qml).
 */
Page {
    id: page

    /// No quick actions; tapped notification waits while offer up: jump leaves other device
    /// half-copied. See piirit.qml. Not once over.
    readonly property bool pausesQuickActions: device.running

    /// Profile offered (core provider's account).
    property int accountId

    /// Chat list's profile: forward swipe target after a device takes this one. Not
    /// [[accountId]]: any profile offers without leaving its chats.
    property int currentAccountId: 0

    property string errorMessage: ""
    /// Device read code, has profile.
    property bool taken: false
    /// Chats attached right: swipe to offer.
    property bool chatsAttached: false
    /// Code shown once; start never reaching code doesn't count.
    property bool shown: false

    /// Core preparing provider.
    readonly property bool preparing: device.running && device.code.length === 0
    /// Code up, unread.
    readonly property bool showing: device.running && device.code.length > 0
                                    && device.permille === 0
    /// Device reading profile off this one.
    readonly property bool transferring: device.running && device.permille > 0

    // Code walkable away from; running transfer not (half-copied device): Cancel out.
    backNavigation: !page.transferring

    // Top row: name, picture, address, as profiles page.
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
        // Provider ended, nothing went: core has no words. As restore's "stalled": both ends quit
        // on same silence.
        onStalled: page.errorMessage = qsTr("No device copied the profile. Both devices must stay on the same network with this page open.")
        onError: page.errorMessage = message
        // Button text: code ever shown, not offer started.
        onCode_changed: {
            if (device.code.length > 0) {
                page.shown = true
            }
        }
    }

    // Core reports transfer like import/export, unnamed; listen only while asked. See
    // second_device.rs.
    Connections {
        target: core
        onCore_event: device.handle_event(context_id, kind, payload_json)
    }

    // Code picture for other phone's camera.
    QrCode {
        id: qr
        objectName: "qr"
        text: device.code
    }

    // Ended on any leave: left provider holds profile out unseen. Cancel stops without leaving.
    // Covered (call page over) is not leaving.
    Component.onDestruction: device.cancel()

    /// Start offer on reader's word. No core check: absent core answers "not started" on
    /// `error`, a banner, better than silent button.
    function begin() {
        if (device.running) {
            return
        }
        page.errorMessage = ""
        page.taken = false
        device.offer()
    }

    /// Chats right: swipe leaves. Attached, not pushed.
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

            // Profile other device gets, drawn as profiles page.
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

            // What to do on other device: only thing reader can't work out here.
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

            // What button shows, said before press: last check of who's in room.
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

            // No code while core starts provider.
            BusyIndicator {
                objectName: "preparing"
                anchors.horizontalCenter: parent.horizontalCenter
                running: page.preparing
                visible: page.preparing
                size: BusyIndicatorSize.Medium
            }

            // White whatever ambience, unsmoothed: see QrPage.qml.
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

            // Same string for camera that won't read it: RestoreProfilePage takes it typed/pasted.
            Label {
                objectName: "codeLabel"
                visible: page.showing
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.WrapAnywhere
                // Core's payload, pinned plain.
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

            // Transfer shown; code no longer subject.
            ProgressBar {
                objectName: "transferProgress"
                width: parent.width
                visible: page.transferring
                minimumValue: 0
                maximumValue: 1000
                value: device.permille
                label: qsTr("Transferring…")
            }

            // Cancel: stops offer, stays; button then "Show code again".
            Button {
                objectName: "cancelButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: device.running
                text: qsTr("Cancel")
                onClicked: device.cancel()
            }

            // Lone action: code up, again later. Gone while offer runs or once taken.
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

            // Way on, words and indicator: only when page exists right.
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

    // Errors: core refused offer or code failed. Strip, not page: retry stays.
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
