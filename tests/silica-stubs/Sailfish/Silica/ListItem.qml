import QtQuick 2.0
Item {
    id: root

    property int contentHeight: 80
    property bool down: false
    property bool highlighted: down
    property bool menuOpen: false
    property var menu
    // Component built on set (Silica: on first open), in declaring scope:
    // tests look inside a row's menu as soon as the row exists.
    onMenuChanged: {
        if (menu && typeof menu.createObject === "function") {
            menu = menu.createObject(root)
        }
    }
    // Row grows by menu height while open. Lists that move on content growth
    // must move on menu open. Height is stub's own number.
    property real menuHeight: 400
    signal clicked()
    // Silica opens context menu on long press.
    function openMenu() { menuOpen = true }
    // Closes on menu item tap or tap away.
    function closeMenu() { menuOpen = false }

    // No `remorseAction`: once modelled, but its behaviour when the row dies
    // mid-countdown can't be stubbed honestly. qml/ no longer calls it:
    // destroy-wait is components/PendingRemoval.qml. Left out so reaching for
    // it fails here, not on a phone.

    width: parent ? parent.width : 540
    height: contentHeight + (menuOpen ? menuHeight : 0)
}
