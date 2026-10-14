import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * One profile written to a file: other half of taking a profile over
 * (RestoreProfilePage.qml).
 *
 * Export is per profile: takes an account, writes its messages, contacts and
 * key into one .tar. Reached from the profile's own row; three profiles, three
 * backups.
 *
 * File goes to Documents under core's name; readers don't type paths. Path
 * shown in full after: next step is copying it off phone.
 *
 * Write is core's, progress via shim (`backup.rs`): bar and stop. Error = strip
 * at bottom; retry is the button still there.
 *
 * Once written: button goes; chats attach right (the app's current profile,
 * not necessarily this one).
 */
Page {
    id: page

    /// No quick actions, tapped notification waits, while writing: jump away
    /// leaves it half-written. See piirit.qml.
    readonly property bool pausesQuickActions: backup.running

    /// Account backed up: export takes one.
    property int accountId

    /// Chat list's profile, where forward swipe goes. Not [[accountId]]:
    /// any profile backs up without leaving its chats.
    property int currentAccountId: 0

    /// Where file went, once written.
    property string writtenPath: ""
    property string errorMessage: ""
    /// Chats on the right: a swipe to offer.
    property bool chatsAttached: false

    // No going back mid-write: would drop answer listener; core keeps writing.
    backNavigation: !backup.running

    // Row at top: name, picture, address, as profiles page draws them.
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

    // Export reports like import, event doesn't say which; object listens only
    // while it asked. See backup.rs.
    Connections {
        target: core
        onCore_event: backup.handle_event(context_id, kind, payload_json)
    }

    function begin() {
        page.errorMessage = ""
        page.writtenPath = ""
        backup.write(StandardPaths.documents)
    }

    /// Chats to the right of this page, so a swipe leaves it.
    ///
    /// Attached, not pushed: reader is done here; page indicator is the offer.
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
                title: qsTr("Back up profile")
            }

            // Row like profiles page: picture, name, address.
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
                text: qsTr("Saves your messages, contacts and end-to-end encryption setup to a backup file. Keep this file in a safe place.")
            }

            Button {
                objectName: "writeButton"
                anchors.horizontalCenter: parent.horizontalCenter
                // Gone once written: a second backup is pointless.
                visible: !backup.running && page.writtenPath.length === 0
                text: qsTr("Start backup")
                onClicked: page.begin()
            }

            ProgressBar {
                objectName: "writeProgress"
                width: parent.width
                visible: backup.running
                minimumValue: 0
                maximumValue: 1000
                value: backup.permille
                label: qsTr("Backing up…")
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

            // Whole path: reader goes looking with file manager or cable.
            Label {
                objectName: "writtenLabel"
                visible: page.writtenPath.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.WrapAnywhere
                // Path from filesystem: plain text, like all text app didn't write.
                textFormat: Text.PlainText
                color: Theme.highlightColor
                //: Said once the backup is written. %1 is the full path
                //: of the file.
                text: qsTr("Saved to %1").arg(page.writtenPath)
            }

            // The way on, in words: only when a page is really there.
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

    // Error: export refused, or folder unwritable. Strip: button stays.
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
