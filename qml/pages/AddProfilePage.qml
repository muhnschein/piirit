import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Three ways a profile gets onto phone, asked at once. Opened from plus under
 * profiles.
 *
 * Same three as first screen gives no-profile readers (ProfileStartPage.qml,
 * ExistingProfilePage.qml).
 *
 * Does nothing itself: AddProfileDialog picks relay, hands to setup;
 * RestoreProfilePage does both transfers, `from` says which.
 */
Page {
    id: page

    /// No quick actions while up: first step of adding profile. See piirit.qml.
    readonly property bool pausesQuickActions: true

    function takeOver(from) {
        pageStack.push(Qt.resolvedUrl("RestoreProfilePage.qml"), { from: from })
    }

    SilicaFlickable {
        anchors.fill: parent
        // Holds tiles; never shorter than page.
        contentHeight: Math.max(height, ways.y + ways.height + Theme.paddingLarge)

        PageHeader {
            id: header
            objectName: "header"
            title: qsTr("Add profile")
        }

        // Stacked: a third of screen is too narrow for two lines.
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
                    // Waits for core; relay dialog goes to setup.
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
