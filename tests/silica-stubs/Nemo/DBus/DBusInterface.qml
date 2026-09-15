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
    // `enum Status { Unknown, Unavailable, Available }` on the real type.
    // Left at Unknown: a test has no bus, so nothing it watches is ever
    // available, and the calls that would follow are driven directly.
    property int status: 0

    // The real one puts the call on the bus and hands the reply to the
    // callback. Here there is no bus and so no reply: a test drives what
    // the reply would have led to itself.
    function typedCall(method, args, onSuccess, onError) {}
}
