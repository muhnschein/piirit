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
 * key into one .tar. So this is reached from the profile's own page
 * rather than from the settings that belong to no profile, and the page
 * says whose backup it is: a phone with three profiles on it makes three
 * backups, one at a time.
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
 */
Page {
    id: page

    /// Whose backup. The core's export takes an account, and this is it.
    property int accountId

    /// The address the profile writes from, for saying whose backup this
    /// is. Empty leaves the page saying it without a name.
    property string address: ""

    /// Where the file went, once one has been written.
    property string writtenPath: ""
    property string errorMessage: ""

    // Nothing to go back to mid-write: leaving would drop the page that
    // is listening for the answer, and the core would carry on writing.
    backNavigation: !backup.running

    Backup {
        id: backup
        objectName: "backup"
        account_id: page.accountId
        onWritten: {
            page.errorMessage = ""
            page.writtenPath = path
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

            // Whose backup: the address is what tells two profiles
            // apart. In a Label of its own rather than the header's
            // description, for the reason ConversationHeader exists --
            // Silica's header cannot be told its text is plain.
            Label {
                objectName: "addressLabel"
                visible: page.address.length > 0
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.WrapAnywhere
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
                text: page.address
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
                text: qsTr("This profile's messages, contacts and key in one file, saved to Documents. Whoever has that file has the profile, so keep it somewhere safe. Your other profiles are not in it: each one is backed up from its own page.")
            }

            Button {
                objectName: "writeButton"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !backup.running
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
