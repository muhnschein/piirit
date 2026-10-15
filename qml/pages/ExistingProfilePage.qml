import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Existing profile: other device (local network, both keep it) or backup
 * file. Asks which (camera vs file browser), then RestoreProfilePage.
 */
Page {
    id: page

    /// No quick actions: profile transfer step. See piirit.qml.
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

        // Each answer pictured: phone, file.
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
