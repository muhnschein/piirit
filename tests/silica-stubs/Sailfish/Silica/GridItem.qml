import QtQuick 2.0

// Grid cell with menu, as ListItem is for lists. Sized by
// `contentWidth`/`contentHeight` (Silica reads them from view cells). Menu
// built and found as ListItem stub does.
Item {
    id: root

    property real contentWidth: 180
    property real contentHeight: 180
    property bool down: false
    property bool highlighted: down
    property bool menuOpen: false
    property bool showMenuOnPressAndHold: true
    property var menu
    onMenuChanged: {
        if (menu && typeof menu.createObject === "function") {
            menu = menu.createObject(root)
        }
    }
    signal clicked()
    function openMenu() { menuOpen = true }
    function closeMenu() { menuOpen = false }

    width: contentWidth
    height: contentHeight
}
