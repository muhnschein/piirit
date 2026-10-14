import QtQuick 2.0

// Silica flickable sizes content to own width; Qt leaves contentWidth 0:
// anchored children 0px, list builds no delegates. Without this stub disagrees
// with device.
Flickable {
    property bool pressDelay
    property var pullDownMenu
    contentWidth: width
}
