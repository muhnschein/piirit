import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Plus (or other theme icon) where a picture would be. No backing: plus has its own ring.
 * Avatar-sized space, so text lines up.
 */
Item {
    id: mark

    /// Theme icon; default the plus.
    property alias source: icon.source

    width: Theme.itemSizeSmall
    height: width

    Image {
        id: icon
        objectName: "plusIcon"
        anchors.centerIn: parent
        source: "image://theme/icon-m-add"
    }
}
