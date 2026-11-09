import QtQuick 2.0

// Label plus clicked(), which harness emits for tap. `icon` aliases real item, as
// IconButton's.
Item {
    property string text
    property bool down: false
    // Colour of label and outline.
    property color color
    property alias icon: iconImage
    Image { id: iconImage; visible: false }
    signal clicked()
    implicitWidth: 200
    implicitHeight: 60
}
