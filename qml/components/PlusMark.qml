import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The mark on a row that adds one more of something -- a member, a
 * profile -- standing where the picture of the thing itself stands on
 * the rows above it.
 *
 * The theme's own plus, and nothing behind it: that icon carries a ring
 * of its own, so an avatar-shaped disc behind it reads as a circle
 * inside a circle. The space is still an avatar's, so the text beside it
 * lines up with the names above.
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
