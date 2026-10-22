import QtQuick 2.0
Item {
    property string title
    property string description
    // Header's interior, left of title: what pages put there.
    property alias extraContent: extra
    implicitHeight: 100
    Item {
        id: extra
        anchors.fill: parent
    }
}
