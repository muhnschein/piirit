import QtQuick 2.0
Item {
    id: root

    property int contentHeight: 80
    property bool down: false
    property bool highlighted: down
    property bool menuOpen: false
    property var menu
    // Silica takes a Component here as well as a menu, and builds the
    // Component the first time the menu is opened. The tests look inside
    // a row's menu the moment the row exists, so a Component is built
    // here as soon as it is set, in the scope it was declared in -- which
    // is what Silica's own build gives it.
    onMenuChanged: {
        if (menu && typeof menu.createObject === "function") {
            menu = menu.createObject(root)
        }
    }
    // How much taller the row stands while its menu is open.
    //
    // Silica's menu unfolds *inside* the row -- the row is its content
    // with the menu under it -- so opening one grows the view's content
    // by a menu's height, over the unfold's animation. That growth is
    // the whole reason this is modelled: a list that moves itself when
    // its content changes height moves itself when a menu opens, and a
    // stub whose rows never grew could not show it. The height is the
    // stub's own number, since a menu here has no layout to measure.
    property real menuHeight: 400
    signal clicked()
    // Silica opens the row's context menu on a long press, and offers
    // this for anything that took the press itself and wants the same.
    function openMenu() { menuOpen = true }
    // And closes it when a menu item is tapped, or the reader taps away.
    function closeMenu() { menuOpen = false }

    // No `remorseAction`. Silica has one and this stub used to model it,
    // guessing at what it does when the row is destroyed mid-countdown --
    // which is exactly the case that matters, and exactly the one a stub
    // cannot answer for. Nothing in qml/ calls it any more: a wait before
    // something is destroyed belongs to the list (components/
    // PendingRemoval.qml), so a row going away is not part of it. Left out
    // rather than left in, so anything that reaches for it again fails
    // here rather than on a phone.

    width: parent ? parent.width : 540
    height: contentHeight + (menuOpen ? menuHeight : 0)
}
