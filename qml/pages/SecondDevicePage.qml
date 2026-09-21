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
 * The offer starts with the page rather than behind a button. There is
 * nothing else to do here -- the reader came from a menu item that says
 * what this is -- and a code is what they came for; the button that is
 * here instead is the one that tries again after a failure. Cancel is
 * the way out while it is up, because leaving would drop the page
 * listening for the answer and leave a provider running behind it.
 *
 * Two things about the core are said on the page rather than left to be
 * found out. The profile stops fetching mail while the offer is up --
 * the core pauses its IO for as long as the provider runs, and resumes
 * it when the provider ends. And whoever reads the code gets the
 * profile, key and all, which is the same warning the backup file's
 * page carries about the file.
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
    /// An offer has been started at least once. Until then there is
    /// nothing to try again -- the page is on its way up, or waiting
    /// for a core that is still starting.
    property bool started: false

    /// The core is preparing the provider and there is nothing to show
    /// yet.
    readonly property bool preparing: device.running && device.code.length === 0
    /// The code is up and nobody has read it yet.
    readonly property bool showing: device.running && device.code.length > 0
                                    && device.permille === 0
    /// A device is reading the profile off this one.
    readonly property bool transferring: device.running && device.permille > 0

    // Nothing to go back to while the offer is up: leaving would drop
    // the page that is listening for the answer, and the core would
    // carry on holding the profile out with its IO paused.
    backNavigation: !device.running

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
        onStalled: page.errorMessage = qsTr("Nothing took the profile. Both phones have to stay on one network, with this page open.")
        onError: page.errorMessage = message
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

    // Offered as soon as the page is up, and again if the core was not
    // ready when it was: a page opened while the app is still starting
    // has nothing to ask.
    Component.onCompleted: page.begin()

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core
        onStatus_changed: {
            if (core.status === "ready") {
                page.begin()
            }
        }
    }

    /// Start the offer, once there is a core to start it on.
    function begin() {
        if (core.status !== "ready" || device.running) {
            return
        }
        page.errorMessage = ""
        page.taken = false
        page.started = true
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
                text: qsTr("On the other device: add a profile you have already, then \"Add as second device\", and read this code with it. Both phones on one network.")
            }

            // Two things about the core, said rather than found out: the
            // profile pauses while the offer is up, and the code is the
            // profile.
            Label {
                objectName: "caution"
                visible: !page.taken
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: qsTr("Whoever reads this code gets the profile. It stops collecting mail until the code is gone.")
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
                label: qsTr("Handing the profile over...")
            }

            Button {
                objectName: "cancelButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: device.running
                text: qsTr("Cancel")
                onClicked: {
                    device.cancel()
                    pageStack.pop()
                }
            }

            // The offer is over and nothing came of it: what went wrong
            // is in the strip along the bottom, and this is the way to
            // have another go.
            Button {
                objectName: "retryButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: page.started && !device.running && !page.taken
                text: qsTr("Show the code again")
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
                text: qsTr("The other device has the profile. Both get everything new from now on.")
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
