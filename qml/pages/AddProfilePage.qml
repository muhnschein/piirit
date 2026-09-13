import QtQuick 2.0
import Sailfish.Silica 1.0

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
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Add profile")
            }

            Button {
                objectName: "createProfileButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Create a profile")
                // Nothing to create with until the core is up, and the
                // relay dialog hands straight over to the setup.
                enabled: core.status === "ready"
                onClicked: pageStack.push(Qt.resolvedUrl("AddProfileDialog.qml"), {})
            }

            Label {
                objectName: "createProfileHint"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("A new address on a chatmail relay.")
            }

            Item { width: 1; height: Theme.paddingLarge }

            Button {
                objectName: "backupFileButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Restore from a backup")
                onClicked: page.takeOver("file")
            }

            Label {
                objectName: "backupFileHint"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("A backup file copied onto this phone.")
            }

            Item { width: 1; height: Theme.paddingLarge }

            Button {
                objectName: "secondDeviceButton"
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Add as second device")
                onClicked: page.takeOver("device")
            }

            Label {
                objectName: "secondDeviceHint"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("The other device keeps it. Both get everything new.")
            }
        }
    }
}
