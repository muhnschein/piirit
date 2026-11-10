import QtQuick 2.0
Item {
    property string title
    property string description
    // Header's interior, left of title: what pages put there.
    property alias extraContent: extra
    // Title's place: below centre, as on device; pages line up beside it.
    property real topMargin: 40
    readonly property real titleHeight: 40
    implicitHeight: 100
    Item {
        id: extra
        anchors.fill: parent
    }
}
