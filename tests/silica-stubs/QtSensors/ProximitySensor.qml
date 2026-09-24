import QtQuick 2.0

// The device's proximity sensor. Nothing comes near here: a test hands it
// a reading through `report`, with ProximityReading's own `near`, the way
// the hardware would. A new object is a new reading, so assigning it
// raises readingChanged.
QtObject {
    property bool active: false
    property var reading: null
    function report(near) { reading = { near: near } }
}
