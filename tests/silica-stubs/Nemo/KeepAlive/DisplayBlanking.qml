import QtQuick 2.0

// Stands in for Nemo.KeepAlive's DisplayBlanking, which on a device asks
// mce to keep the display on while `preventBlanking` holds. Nothing is
// lit here: it holds whether it would be, for a test to read.
QtObject {
    property bool preventBlanking: false
    property int status: 0
}
