import QtQuick 2.0

// Silica ValueButton: label, current value, tap opens chooser.
Item {
    property string label
    property string value
    property string description
    signal clicked()
    implicitWidth: 400
    implicitHeight: 60
}
