import QtQuick 2.0

// Stands in for Nemo.DBus's DBusInterface, which on a device watches a
// remote object on the bus and calls the function named after each signal
// it hears. Nothing is connected here: it holds the address it would
// watch, and a test calls the component's own entry point instead of a
// bus nobody has in a test.
QtObject {
    property int bus
    property string service
    property string path
    property string iface
    property bool signalsEnabled
    property bool watchServiceStatus
}
