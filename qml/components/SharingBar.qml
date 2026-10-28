import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Chat gets reader's location live: says so above field, stop beside. Own file so it loads
 * and tests alone (ConversationPage won't headless).
 */
Item {
    id: root

    /// Chat gets location now.
    property bool sharing: false
    signal stopRequested()

    visible: root.sharing
    height: root.sharing ? Math.max(label.height, stop.height) + 2 * Theme.paddingSmall : 0

    Image {
        id: pin
        anchors.verticalCenter: parent.verticalCenter
        x: Theme.horizontalPageMargin
        width: Theme.iconSizeSmall
        height: Theme.iconSizeSmall
        source: "image://theme/icon-m-location?" + Theme.highlightColor
    }

    Label {
        id: label
        objectName: "sharingLabel"
        anchors.verticalCenter: parent.verticalCenter
        x: pin.x + pin.width + Theme.paddingSmall
        width: parent.width - x - stop.width - Theme.paddingMedium
        wrapMode: Text.Wrap
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.highlightColor
        textFormat: Text.PlainText
        text: qsTr("You are sharing your location")
    }

    IconButton {
        id: stop
        objectName: "stopSharingButton"
        anchors {
            verticalCenter: parent.verticalCenter
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        icon.source: "image://theme/icon-m-clear"
        onClicked: root.stopRequested()
    }
}
