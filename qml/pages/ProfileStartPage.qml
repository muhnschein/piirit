import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Setup path start: from first screen and end of IntroPage walk. Create: relay
 * dialog. Existing profile: ExistingProfilePage.qml.
 */
Page {
    id: page

    allowedOrientations: Orientation.All

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
            text: qsTr("Set up your profile")
            font.family: Theme.fontFamilyHeading
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.highlightColor
        }

        Item { width: 1; height: Theme.paddingLarge }

        // Two tiles: transfer arrows (profile exists elsewhere), plus (new).
        ChoiceTiles {
            objectName: "startWays"
            width: parent.width
            // Column already inside page margins.
            sideMargin: 0
            choices: [
                {
                    name: "existingProfile",
                    icon: "icon-m-transfer",
                    text: qsTr("I already have a profile")
                },
                {
                    name: "createProfile",
                    icon: "icon-m-add",
                    text: qsTr("Create a profile"),
                    enabled: core.status === "ready"
                }
            ]
            onChosen: {
                if (name === "existingProfile") {
                    pageStack.push(Qt.resolvedUrl("ExistingProfilePage.qml"), {})
                } else {
                    pageStack.push(Qt.resolvedUrl("AddProfileDialog.qml"), {})
                }
            }
        }
    }
}
