import QtQuick 2.0

// `icon` must alias real item: QML takes `icon.source:` only for value type or
// alias.
Item {
    property alias icon: iconImage
    Image { id: iconImage; visible: false }
    signal clicked()
    // Silica's: MouseArea raising this on long press.
    signal pressAndHold()
    // Explicit, not implicit: plain Item's implicitHeight leaves `height`
    // unset.
    implicitWidth: 60
    implicitHeight: 60
    width: 60
    height: 60
}
