import QtQuick 2.0

// Silica slider, cut to video page's needs. `down` true while handle held:
// keeps position binding off reader's drag value.
Item {
    id: root

    property real minimumValue: 0
    property real maximumValue: 1
    property real value: 0
    property real stepSize: 0
    property string label
    property string valueText
    property bool down: false
    property bool handleVisible: true

    signal released()

    width: parent ? parent.width : 540
    height: 80
}
