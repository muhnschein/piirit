import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Profile exists elsewhere: on a device they have (local network, both keep
 * it) or in a backup file copied here. Both go to RestoreProfilePage. This
 * page asks which: first steps differ (camera vs file browser).
 */
Page {
    id: page

    /// No quick actions while up: first step of a profile transfer.
    /// See piirit.qml.
    readonly property bool pausesQuickActions: true

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

        // Each answer shows what it is about: other phone, file it wrote.
        ChoiceTiles {
            objectName: "whereWays"
            width: parent.width
            // Already inside margins.
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
