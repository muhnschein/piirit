import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * The three ways a profile gets onto this phone, asked as one question.
 * Reached from the plus under the profiles list.
 *
 * The same three the first screen offers a reader with no profile at all,
 * over two pages there (ProfileStartPage.qml, ExistingProfilePage.qml):
 * make one on a chatmail relay, read one back from a backup file, or join
 * one that is running on another device right now. Somebody adding a
 * second profile has already been through that walk once, so this asks
 * all three at once rather than walking them again.
 *
 * Nothing happens here: each way is a page that does the work.
 * AddProfileDialog picks the relay and hands over to the setup;
 * RestoreProfilePage does both transfers, and which one is its `from`.
 */
Page {
    id: page

    function takeOver(from) {
        pageStack.push(Qt.resolvedUrl("RestoreProfilePage.qml"), { from: from })
    }

    SilicaFlickable {
        anchors.fill: parent
        // Long enough to hold the tiles, and never shorter than the
        // page itself.
        contentHeight: Math.max(height, ways.y + ways.height + Theme.paddingLarge)

        PageHeader {
            id: header
            objectName: "header"
            title: qsTr("Add profile")
        }

        // The three ways, one under another under the header. Three
        // tiles in a row left each of them a third of the screen, which
        // is not room for a line of words and the line under it;
        // stacked, each is a row with its icon at the left and its two
        // lines beside it, and the eye goes down them one at a time --
        // which is the reading order everything else on the phone has,
        // and it starts at the top.
        ChoiceTiles {
            id: ways
            objectName: "addWays"
            stacked: true
            width: page.width
            y: header.height + Theme.paddingLarge
            choices: [
                {
                    name: "createProfile",
                    icon: "icon-m-add",
                    text: qsTr("Create a profile"),
                    // Nothing to create with until the core is up, and
                    // the relay dialog hands straight over to the setup.
                    enabled: core.status === "ready"
                },
                {
                    name: "backupFile",
                    icon: "icon-m-backup",
                    text: qsTr("Restore from a backup")
                },
                {
                    name: "secondDevice",
                    icon: "icon-m-device",
                    text: qsTr("Add as second device")
                }
            ]
            onChosen: {
                if (name === "createProfile") {
                    pageStack.push(Qt.resolvedUrl("AddProfileDialog.qml"), {})
                } else {
                    page.takeOver(name === "backupFile" ? "file" : "device")
                }
            }
        }
    }
}
