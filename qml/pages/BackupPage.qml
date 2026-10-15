import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * One profile to file; other half of takeover (RestoreProfilePage.qml). Per
 * profile: messages, contacts, key into one .tar; reached from profile's row.
 * To Documents under core's name (readers don't type paths); full path shown
 * after, for copying off phone. Core writes, shim reports (`backup.rs`): bar,
 * stop. Error: bottom strip, button retries. Done: button goes, chats attach
 * right (app's current profile, maybe not this).
 */
Page {
    id: page

    /// While writing no quick actions, tapped notification waits: jump leaves it half-written.
    /// piirit.qml.
    readonly property bool pausesQuickActions: backup.running

    /// Account backed up.
    property int accountId

    /// Chat list's profile, forward swipe target. Not [[accountId]]: backing up any profile
    /// doesn't leave current chats.
    property int currentAccountId: 0

    /// Where file went, once written.
    property string writtenPath: ""
    property string errorMessage: ""
    /// Chats attached right.
    property bool chatsAttached: false

    // No back mid-write: drops listener; core keeps writing.
    backNavigation: !backup.running

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

    // Export reports like import, event doesn't say which: listens only while asked (backup.rs).
    Connections {
        target: core
        onCore_event: backup.handle_event(context_id, kind, payload_json)
    }

    function begin() {
        page.errorMessage = ""
        page.writtenPath = ""
        backup.write(StandardPaths.documents)
    }

    /// Attach chats right, not pushed: reader done; page indicator is the offer.
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

            // As profiles page row: picture, name, address.
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
                // Gone once written: second is pointless.
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

            // Whole path, for file manager or cable.
            Label {
                objectName: "writtenLabel"
                visible: page.writtenPath.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.WrapAnywhere
                // Filesystem path: plain text, as all foreign text.
                textFormat: Text.PlainText
                color: Theme.highlightColor
                //: Said once the backup is written. %1 is the full path
                //: of the file.
                text: qsTr("Saved to %1").arg(page.writtenPath)
            }

            // Way on, in words: only when page really there.
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

    // Export refused or folder unwritable. Strip: button stays.
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
