import QtQuick 2.0

// Label plus clicked(), which harness emits for tap.
Item {
    property string text
    property bool down: false
    // Colour of label and outline.
    property color color
    signal clicked()
    implicitWidth: 200
    implicitHeight: 60
}
