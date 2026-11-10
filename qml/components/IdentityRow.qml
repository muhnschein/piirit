import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Contact or group on its page: picture left, facts beside it (lines given by page),
 * centred on each other; as ContactRow, larger. Tap enlarges a real picture.
 */
Item {
    id: root

    property alias initial: avatar.initial
    property alias ownColor: avatar.ownColor
    property alias picturePath: avatar.picturePath
    default property alias lines: lines.data

    signal pictureClicked()

    /// Picture's top in row; as much room under it (centred).
    readonly property real pictureTop: avatar.y

    width: parent ? parent.width : 0
    height: Math.max(avatar.height, lines.height) + 2 * Theme.paddingLarge

    Avatar {
        id: avatar
        objectName: "identityAvatar"
        x: Theme.horizontalPageMargin
        anchors.verticalCenter: parent.verticalCenter
        width: Theme.itemSizeExtraLarge
    }

    MouseArea {
        objectName: "pictureTap"
        anchors.fill: avatar
        enabled: avatar.picturePath.length > 0
        onClicked: root.pictureClicked()
    }

    Column {
        id: lines
        x: avatar.x + avatar.width + Theme.paddingLarge
        width: root.width - x - Theme.horizontalPageMargin
        anchors.verticalCenter: parent.verticalCenter
        spacing: Theme.paddingSmall
    }
}
