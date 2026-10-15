import QtQuick 2.0

// Stub: orientation sensor. Test sets reading via `report`: TopUp 1, TopDown 2,
// LeftUp 3, RightUp 4. New object = new reading.
QtObject {
    property bool active: false
    property var reading: null
    function report(orientation) { reading = { orientation: orientation } }
}
