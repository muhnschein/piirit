import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * This phone offering one of its profiles to another device: the other
 * end of RestoreProfilePage's `from: "device"`, and the end Piirit did
 * not have. A Piirit could be added to a setup somebody else's Delta
 * Chat was holding, but could not be the one holding it -- so a phone
 * with nothing else in the house had a profile no second device could
 * ever join.
 *
 * The transfer is the core's, both ways round: the provider offers
 * itself on the local network and shows a code, the other device reads
 * it and takes a copy, and both end up with the profile. So this page is
 * the code, and the bar the transfer draws once somebody reads it.
 *
 * The code waits behind a button. Nothing on the phone can tell a
 * second device of the reader's own from a camera over their shoulder,
 * and what this page shows is the profile -- messages, contacts and key
 * -- so the reader is the one who says when it goes up, with the line
 * above the button saying what it is they are putting on screen. That
 * is also why the button does not start the offer on the way in: a page
 * opened by mistake shows nothing worth reading.
 *
 * The button says what it is about to do: "Show code" before there has
 * been one, "Show code again" once the reader has seen one -- after
 * Cancel, after the app has been away, or after an offer nobody came
 * for. Cancel takes the code down and leaves the reader here, on the
 * button, rather than off the page: that is what stopping the code
 * means when the page is a place of its own.
 *
 * Leaving the page ends the offer. A provider left running behind a
 * page that is gone holds the profile out with nobody watching and with
 * its IO paused, so going back does what Cancel does. Only a transfer
 * actually under way pins the page: a bar that is filling is a second
 * device part-way through copying the profile, and a stray swipe should
 * not drop that -- Cancel, which says what it is, still can.
 *
 * One thing about the core is said here rather than left to be found
 * out: the profile stops fetching mail while the offer is up, because
 * the core pauses its IO for as long as the provider runs and resumes
 * it when the provider ends.
 *
 * The code is drawn from an Image the shim writes, not a Canvas, for
 * the reason QrPage.qml gives: a Canvas is drawn into the window's GL
 * context, which the platform takes away from an app in the background,
 * and it came back blank.
 */
Page {
    id: page

    /// Whose profile is offered. The core's provider takes an account,
    /// and this is it.
    property int accountId

    /// The profile the chat list is on, where the forward swipe goes
    /// once a device has taken this one. Not [[accountId]]: any profile
    /// can be offered from the profiles page without the chats leaving
    /// the one they are on.
    property int currentAccountId: 0

    property string errorMessage: ""
    /// A device read the code and has the profile now.
    property bool taken: false
    /// The chats are attached to the right, so there is a swipe to offer.
    property bool chatsAttached: false
    /// A code has been on screen at least once. Until then there is
    /// nothing to show *again*, so the button says what it means: a
    /// start that never got as far as a code does not count.
    property bool shown: false

    /// The core is preparing the provider and there is nothing to show
    /// yet.
    readonly property bool preparing: device.running && device.code.length === 0
    /// The code is up and nobody has read it yet.
    readonly property bool showing: device.running && device.code.length > 0
                                    && device.permille === 0
    /// A device is reading the profile off this one.
    readonly property bool transferring: device.running && device.permille > 0

    // The code can always be walked away from; a transfer that is
    // already running cannot, because a swipe is too easy a way to drop
    // a second device half-way through copying the profile. Cancel is
    // the way out of that one, and says so.
    backNavigation: !page.transferring

    // Whose profile, for the row at the top: the name, the picture and
    // the address, as the profiles page and the backup page draw them.
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
        // The provider ended and nothing went over: the core has no
        // words for that, so they are here. Same shape as the restore
        // page's "stalled", and for the same reason -- both ends of one
        // transfer, both giving up on the same silence.
        onStalled: page.errorMessage = qsTr("No device copied the profile. Both devices must stay on the same network with this page open.")
        onError: page.errorMessage = message
        // What the button says next time depends on whether there has
        // ever been a code to look at, rather than on whether an offer
        // was started: a provider that failed before it had one showed
        // the reader nothing.
        onCode_changed: {
            if (device.code.length > 0) {
                page.shown = true
            }
        }
    }

    // The core reports the transfer the way it reports an import or an
    // export, and says nothing in the event about which it is; the
    // object only listens while it is the one that asked. See
    // second_device.rs.
    Connections {
        target: core
        onCore_event: device.handle_event(context_id, kind, payload_json)
    }

    // The code as a picture, for the other phone's camera.
    QrCode {
        id: qr
        objectName: "qr"
        text: device.code
    }

    // Ended when the page is left, whichever way. The provider is the
    // core's and outlives this page; left running it would hold the
    // profile out to whoever asks, with nothing on screen to say so and
    // nothing left to stop it. Cancel stops it without leaving; this is
    // the swipe. Harmless on the way to the chats after a hand-over: by
    // then there is no offer to stop.
    onStatusChanged: {
        if (page.status === PageStatus.Deactivating) {
            device.cancel()
        }
    }

    /// Start the offer, on the reader's word.
    ///
    /// No check on the core being up: a core that is not there answers
    /// this with "not started" on `error`, which is a banner the reader
    /// can read, and a button that quietly does nothing is not.
    function begin() {
        if (device.running) {
            return
        }
        page.errorMessage = ""
        page.taken = false
        device.offer()
    }

    /// Put the chats to the right of this page, so a swipe on leaves it.
    ///
    /// Attached rather than pushed: the reader is done here, and the
    /// page indicator saying there is somewhere to go is the offer.
    function offerTheChats() {
        if (page.currentAccountId <= 0 || page.chatsAttached) {
            return
        }
        pageStack.pushAttached(Qt.resolvedUrl("ChatListPage.qml"),
                               { accountId: page.currentAccountId })
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

            // Which profile the other device is about to get, drawn as
            // the profiles page draws a profile: the picture, the name,
            // and under it the address that tells two profiles apart.
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

            // What to do on the other device, which is the only thing
            // the reader cannot work out from this page.
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

            // What pressing the button puts on screen, said before it
            // is pressed: this is the last moment the reader can decide
            // who is in the room.
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

            // While the core is getting the provider up there is no code
            // to draw and nothing to report.
            BusyIndicator {
                objectName: "preparing"
                anchors.horizontalCenter: parent.horizontalCenter
                running: page.preparing
                visible: page.preparing
                size: BusyIndicatorSize.Medium
            }

            // Drawn on white whatever the ambience, and not smoothed:
            // see QrPage.qml for both.
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

            // The same string the code carries, for a camera on the
            // other device that will not read it: RestoreProfilePage
            // takes one typed or pasted in.
            Label {
                objectName: "codeLabel"
                visible: page.showing
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.WrapAnywhere
                // The core's own payload, pinned like everything else
                // this app did not write.
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

            // Once a device is reading the profile off this one there is
            // a transfer to report, and the code is no longer what the
            // page is about.
            ProgressBar {
                objectName: "transferProgress"
                width: parent.width
                visible: page.transferring
                minimumValue: 0
                maximumValue: 1000
                value: device.permille
                label: qsTr("Transferring…")
            }

            // Stops the offer and stays: the page is where the button
            // is, and taking the code down is what Cancel means here.
            // What is left is the button again, saying "Show code
            // again". Leaving the page is the swipe, which ends the
            // offer too.
            Button {
                objectName: "cancelButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: device.running
                text: qsTr("Cancel")
                onClicked: device.cancel()
            }

            // The only thing this page does on its own account: put the
            // code up, and put it up again afterwards. Gone while an
            // offer is running, because Cancel is what that state is
            // answered with, and gone once a device has the profile.
            Button {
                objectName: "showButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !device.running && !page.taken
                text: page.shown ? qsTr("Show code again") : qsTr("Show code")
                onClicked: page.begin()
            }

            // Done: the profile is on the other device too, and both
            // will get everything new from here on.
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

            // The way on, said in words as well as by the indicator:
            // only when there really is a page to the right.
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

    // What went wrong, when something did: the core refusing to offer
    // the profile, or a code it could not produce. The button that
    // tries again is still there, so this is a strip rather than a page
    // of its own.
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
