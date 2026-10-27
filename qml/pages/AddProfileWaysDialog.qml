import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Create, backup or second device; from plus under profiles. One switch at a time, nothing
 * preset (as DeleteMessageDialog.qml); picked page on top, so back returns here.
 */
Dialog {
    id: dialog

    /// No quick actions while up: first step of adding profile. See piirit.qml.
    readonly property bool pausesQuickActions: true

    property string choice: ""

    // Create waits for core; relay dialog goes to setup.
    canAccept: dialog.choice.length > 0 && (dialog.choice !== "create" || core.status === "ready")
    acceptDestination: dialog.choice === "create" ? Qt.resolvedUrl("AddProfileDialog.qml")
                     : dialog.choice.length > 0 ? Qt.resolvedUrl("RestoreProfilePage.qml")
                     : ""
    acceptDestinationProperties: dialog.choice === "create" ? {} : { from: dialog.choice }
    // Silica builds page once per URL: file to device keeps it, so set half again here.
    onAccepted: {
        if (dialog.choice !== "create") {
            dialog.acceptDestinationInstance.from = dialog.choice
        }
    }

    function choose(which) {
        dialog.choice = dialog.choice === which ? "" : which
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column
            width: dialog.width
            spacing: Theme.paddingLarge

            // No title: plus row already says "Add profile".
            DialogHeader {
                objectName: "header"
                //: Goes on to the page for the picked way of adding a profile.
                acceptText: qsTr("Next")
            }

            // `automaticCheck: false`: Silica switch self-flips; one answer, dialog decides.
            TextSwitch {
                objectName: "createSwitch"
                text: qsTr("Create a profile")
                description: qsTr("You start with a new address and no chats.")
                automaticCheck: false
                checked: dialog.choice === "create"
                enabled: core.status === "ready"
                onClicked: dialog.choose("create")
            }

            TextSwitch {
                objectName: "fileSwitch"
                text: qsTr("Restore from a backup")
                description: qsTr("You import a profile from a backup file.")
                automaticCheck: false
                checked: dialog.choice === "file"
                onClicked: dialog.choose("file")
            }

            TextSwitch {
                objectName: "deviceSwitch"
                text: qsTr("Add as second device")
                description: qsTr("You use the same profile on this phone and another device.")
                automaticCheck: false
                checked: dialog.choice === "device"
                onClicked: dialog.choose("device")
            }
        }
    }
}
