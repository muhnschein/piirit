import QtQuick 2.0

// Silica's ValueButton: a label, the current value beside it, and a tap
// that opens whatever chooses a new one.
Item {
    property string label
    property string value
    property string description
    signal clicked()
    implicitWidth: 400
    implicitHeight: 60
}
