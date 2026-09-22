import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * The reader has a profile already, somewhere else. Where it is decides
 * how it gets here, and there are two answers -- the same two the other
 * Delta Chat apps offer:
 *
 * - It is on a device they still have, which can offer it over the local
 *   network. Both devices end up with it; nothing is moved.
 * - It is in a backup file that device wrote, copied onto this phone.
 *
 * Both go to RestoreProfilePage, which does the transfer; this page is
 * the question, because the two need different first steps -- a camera
 * for one, the file browser for the other -- and a reader who has to
 * find that out by trying is a reader who gave up.
 */
Page {
    id: page

    allowedOrientations: Orientation.All

    function takeOver(from) {
        pageStack.push(Qt.resolvedUrl("RestoreProfilePage.qml"), { from: from })
    }

    Column {
        id: words
        anchors {
            horizontalCenter: parent.horizontalCenter
            verticalCenter: parent.verticalCenter
            verticalCenterOffset: -page.height * 0.04
        }
        width: Math.min(parent.width - 2 * Theme.horizontalPageMargin,
                        Screen.width - 2 * Theme.horizontalPageMargin)
        spacing: Theme.paddingMedium

        Label {
            objectName: "lead"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: qsTr("Where is your profile now?")
            font.family: Theme.fontFamilyHeading
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.highlightColor
        }

        Item { width: 1; height: Theme.paddingLarge }

        // Both answers side by side, each with the thing it is about
        // over it: the other phone, and the file it wrote. What was a
        // line of small print under each button is the tile's own second
        // line now, which is where a reader looks for it anyway.
        ChoiceTiles {
            objectName: "whereWays"
            width: parent.width
            // The column is already inside the page's margins.
            sideMargin: 0
            choices: [
                {
                    name: "secondDevice",
                    icon: "icon-m-device",
                    text: qsTr("Add as second device")
                },
                {
                    name: "backupFile",
                    icon: "icon-m-backup",
                    text: qsTr("Restore from a backup")
                }
            ]
            onChosen: page.takeOver(name === "backupFile" ? "file" : "device")
        }
    }
}
