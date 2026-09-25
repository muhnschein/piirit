import QtQuick 2.0

// A label and a clicked() signal the harness can emit for a tap.
Item {
    property string text
    property bool down: false
    // Silica's: the colour the label and the outline are drawn in.
    property color color
    signal clicked()
    implicitWidth: 200
    implicitHeight: 60
}
