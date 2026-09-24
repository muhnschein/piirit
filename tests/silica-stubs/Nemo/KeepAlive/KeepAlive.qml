import QtQuick 2.0

// Stands in for Nemo.KeepAlive's KeepAlive, which on a device keeps the
// CPU from suspending while it is enabled. Nothing is kept awake here: it
// holds whether it would be, for a test to read.
QtObject {
    property bool enabled: false
}
