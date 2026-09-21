import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * The question asked before a profile is held out to another device
 * (SecondDevicePage.qml), and the warning that goes with it.
 *
 * The code the next page shows is the profile: anything that reads it
 * gets the messages, the contacts and the key. Nothing on the phone can
 * tell a second device of the reader's own from a camera over their
 * shoulder, so the only place that can be dealt with is before the code
 * is on screen -- which is what the other Delta Chat apps do too, with
 * an alert in front of their QR screen ("Make sure no unwanted observer
 * or camera can see the following screen").
 *
 * A dialog rather than a banner on the page: Silica's accept and cancel
 * at the top are the answer to a question, the reader has to pass
 * through it on the way, and Cancel here costs nothing because the
 * provider has not been started yet. The profile is drawn as the page
 * draws it, because "which profile" is half of what is being agreed to.
 */
Dialog {
    id: dialog

    /// Whose profile would be offered. Handed to the page on accept.
    property int accountId

    /// The profile the chat list is on, which the page puts to its
    /// right once a device has taken the profile.
    property int currentAccountId: 0

    allowedOrientations: Orientation.All

    // Silica makes the destination as soon as this dialog is on screen;
    // accepting goes to it, so what it needs is filled in here rather
    // than pushed by hand. Same as AddProfileDialog.
    acceptDestination: Qt.resolvedUrl("SecondDevicePage.qml")
    onAccepted: {
        dialog.acceptDestinationInstance.accountId = dialog.accountId
        dialog.acceptDestinationInstance.currentAccountId = dialog.currentAccountId
    }

    // Whose profile, for the row below: the name, the picture and the
    // address, as the profiles page and the backup page draw them.
    Profile {
        id: profile
        objectName: "profile"
        account_id: dialog.accountId
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingLarge

            DialogHeader {
                objectName: "header"
                title: qsTr("Add a second device")
                acceptText: qsTr("Continue")
                cancelText: qsTr("Cancel")
            }

            // Which profile is about to be held out. The same row the
            // next page leads with, so accepting is not a step into
            // something unrecognisable.
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

            Label {
                objectName: "explanation"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                color: Theme.highlightColor
                text: qsTr("This makes a code the other device reads to copy the profile onto itself.")
            }

            // The reason this dialog exists, in the strongest words the
            // page has: a code on screen is the profile in the room.
            Label {
                objectName: "warning"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                color: Theme.errorColor
                text: qsTr("Make sure nobody, and no camera, can see the screen that comes next. Whoever reads that code gets the profile, key and all.")
            }

            Label {
                objectName: "networkHint"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Both phones have to be on the same network, and this profile stops collecting mail while the code is up.")
            }
        }
    }
}
