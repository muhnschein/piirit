import QtQuick 2.0

// Silica row background: press state, clicked(). `down` lets label highlight
// itself.
Item {
    id: root

    property bool down: mouse.pressed
    property bool highlighted: down
    signal clicked()

    width: parent ? parent.width : 540
    height: 80

    MouseArea {
        id: mouse
        anchors.fill: parent
        onClicked: root.clicked()
    }
}
