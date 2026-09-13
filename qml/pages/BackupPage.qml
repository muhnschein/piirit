import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * One profile, written out to a file: the other half of taking a profile
 * over from one (RestoreProfilePage.qml).
 *
 * A backup is the core's export and the core's export is per profile --
 * it takes an account and writes that account's messages, contacts and
 * key into one .tar. So this is reached from the profile's own row on the
 * profiles page rather than from the settings that belong to no profile,
 * and the profile is drawn at the top the way the invite code's page
 * draws it: a phone with three profiles on it makes three backups, and
 * which one this is is the first thing to be sure of.
 *
 * The file goes to Documents under the name the core gives it -- the
 * core names a backup itself, and a reader who has to type a path on a
 * phone is a reader who does not make a backup. Where it landed is said
 * afterwards, in full, because the next thing to do with it is to copy
 * it off the phone.
 *
 * The write is the core's and reports itself in progress events the shim
 * passes on (`backup.rs`), so the middle of this page is a bar and a way
 * to stop. What went wrong, when something does, is a strip along the
 * bottom: unlike a transfer that failed after a minute of bar, the way
 * to try again is the button that is still there.
 *
 * Once a backup has been written there is nothing more to do here, so the
 * button goes and the chats are attached to the right instead: the page
 * ends in a swipe on rather than in an offer to write the same profile
 * out again. The chats attached are the ones the app is on, which is not
 * necessarily this profile -- backing a profile up is not switching to
 * it.
 */
Page {
    id: page

    /// Whose backup. The core's export takes an account, and this is it.
    property int accountId

    /// The profile the chat list is on, where the forward swipe goes.
    /// Not [[accountId]]: any profile can be backed up from the profiles
    /// page without the chats leaving the one they are on.
    property int currentAccountId: 0

    /// Where the file went, once one has been written.
    property string writtenPath: ""
    property string errorMessage: ""
    /// The chats are attached to the right, so there is a swipe to offer.
    property bool chatsAttached: false

    // Nothing to go back to mid-write: leaving would drop the page that
    // is listening for the answer, and the core would carry on writing.
    backNavigation: !backup.running

    // Whose backup this is, for the row at the top: the name, the
    // picture and the address, as the profiles page and the invite code
    // draw them.
    Profile {
        id: profile
        objectName: "profile"
        account_id: page.accountId
    }

    Backup {
        id: backup
        objectName: "backup"
        account_id: page.accountId
        onWritten: {
            page.errorMessage = ""
            page.writtenPath = path
            page.offerTheChats()
        }
        onError: page.errorMessage = message
    }

    // The core reports an export the way it reports an import, and says
    // nothing in the event about which it is; the object only listens
    // while it is the one that asked. See backup.rs.
    Connections {
        target: core
        onCore_event: backup.handle_event(context_id, kind, payload_json)
    }

    function begin() {
        page.errorMessage = ""
        page.writtenPath = ""
        backup.write(StandardPaths.documents)
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
                title: qsTr("Back up profile")
            }

            // Whose backup, drawn as the profiles page draws a profile:
            // the picture, the name, and under it the address, which is
            // what tells two profiles apart.
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
                objectName: "instructions"
                visible: page.writtenPath.length === 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Messages, contacts and key in one file, saved to Documents. Whoever has the file has the profile.")
            }

            Button {
                objectName: "writeButton"
                anchors.horizontalCenter: parent.horizontalCenter
                // Gone once there is a backup: writing the same profile
                // out twice in a row is not what anybody came here for,
                // and the swipe below is what is left to do.
                visible: !backup.running && page.writtenPath.length === 0
                text: qsTr("Write the backup")
                onClicked: page.begin()
            }

            ProgressBar {
                objectName: "writeProgress"
                width: parent.width
                visible: backup.running
                minimumValue: 0
                maximumValue: 1000
                value: backup.permille
                label: qsTr("Writing the backup...")
            }

            Button {
                objectName: "cancelButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: backup.running
                text: qsTr("Cancel")
                onClicked: {
                    backup.cancel()
                    pageStack.pop()
                }
            }

            // Where it went, once it has gone somewhere: the whole path,
            // because the reader is about to go looking for it with a
            // file manager or a cable.
            Label {
                objectName: "writtenLabel"
                visible: page.writtenPath.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.WrapAnywhere
                // A path off the phone's own filesystem, pinned like
                // everything else this app did not write.
                textFormat: Text.PlainText
                color: Theme.highlightColor
                //: Said once the backup is written. %1 is the full path
                //: of the file.
                text: qsTr("Saved to %1").arg(page.writtenPath)
            }

            // The way on, said in words as well as by the indicator:
            // only when there really is a page to the right.
            Label {
                objectName: "onwardHint"
                visible: page.writtenPath.length > 0 && page.chatsAttached
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

    // What went wrong, when something did: the core refusing the export,
    // or a folder it could not write into. The button is still there, so
    // this is a strip rather than a page of its own.
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
