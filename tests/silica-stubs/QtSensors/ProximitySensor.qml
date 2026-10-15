import QtQuick 2.0

// Stub: proximity sensor. Test sets reading via `report` (`near`). New object =
// new reading.
QtObject {
    property bool active: false
    property var reading: null
    function report(near) { reading = { near: near } }
}
