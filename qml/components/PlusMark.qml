import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Plus where a picture would be. No backing: theme icon has its own ring.
 * Avatar-sized space, so text lines up.
 */
Item {
    id: mark

    width: Theme.itemSizeSmall
    height: width

    Image {
        objectName: "plusIcon"
        anchors.centerIn: parent
        source: "image://theme/icon-m-add"
    }
}
