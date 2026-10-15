import QtQuick 2.0

// Silica drop-down: label, chosen item text, ContextMenu of MenuItems. Silica
// sets currentIndex on tap; pages bind it from core, so stub holds properties
// only.
Item {
    property string label
    property string description
    property string value
    property int currentIndex: -1
    property var currentItem
    property var menu
    implicitWidth: 400
    implicitHeight: 80
}
