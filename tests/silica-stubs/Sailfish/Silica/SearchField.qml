import QtQuick 2.0
Item {
    property string text: ""
    property string placeholderText
    property bool canHide: false
    signal clicked()
    signal hideClicked()
    implicitWidth: 400
    implicitHeight: 60
}
