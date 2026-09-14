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
        // Long enough to hold the tiles where they are put, and never
        // shorter than the page: the stack is placed against the page's
        // own height rather than against this, which would be a loop.
        contentHeight: Math.max(height, ways.y + ways.height + Theme.paddingLarge)

        PageHeader {
            id: header
            objectName: "header"
            title: qsTr("Add profile")
        }

        // The three ways, one under another and centred in what the
        // header leaves. Three tiles in a row left each of them a third
        // of the screen, which is not room for a line of words and the
        // line under it; stacked, each gets the width and the eye goes
        // down them one at a time. Centred rather than stacked up under
        // the header: three tall tiles at the top of a long screen read
        // as a list that ran out.
        ChoiceTiles {
            id: ways
            objectName: "addWays"
            stacked: true
            width: page.width
            y: header.height + Math.max(Theme.paddingLarge,
                                        (page.height - header.height
                                         - ways.height) / 2)
            choices: [
                {
                    name: "createProfile",
                    icon: "icon-m-add",
                    text: qsTr("Create a profile"),
                    hint: qsTr("A new address on a chatmail relay."),
                    // Nothing to create with until the core is up, and
                    // the relay dialog hands straight over to the setup.
                    enabled: core.status === "ready"
                },
                {
                    name: "backupFile",
                    icon: "icon-m-backup",
                    text: qsTr("Restore from a backup"),
                    hint: qsTr("A backup file copied onto this phone.")
                },
                {
                    name: "secondDevice",
                    icon: "icon-m-device",
                    text: qsTr("Add as second device"),
                    hint: qsTr("The other device keeps it. Both get everything new.")
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
