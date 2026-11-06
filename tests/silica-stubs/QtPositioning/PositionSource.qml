import QtQuick 2.0

// Stub: phone's position. Finds nothing; test hands fix via `report` (`age` ms old, as Qt's
// last known fix from disk), loses it via `forget`, failure via `fail` (Qt's `sourceError`
// numbers: 0 access, 1 closed, 2 unknown, 3 none). New object = new fix. Unknown accuracy
// NaN, no timestamp invalid Date, as Qt.
QtObject {
    enum SourceError { AccessError, ClosedError, UnknownSourceError, NoError }
    property bool active: false
    property int updateInterval: 0
    property bool valid: true
    property int sourceError: 3
    property var position: ({ latitudeValid: false, longitudeValid: false,
                              horizontalAccuracyValid: false, horizontalAccuracy: NaN,
                              coordinate: { latitude: 0, longitude: 0 },
                              timestamp: new Date(NaN) })
    function report(latitude, longitude, accuracy, age) {
        position = { latitudeValid: true, longitudeValid: true,
                     horizontalAccuracyValid: accuracy > 0,
                     horizontalAccuracy: accuracy > 0 ? accuracy : NaN,
                     coordinate: { latitude: latitude, longitude: longitude },
                     timestamp: new Date(Date.now() - (age > 0 ? age : 0)) }
    }
    function forget() {
        position = { latitudeValid: false, longitudeValid: false,
                     horizontalAccuracyValid: false, horizontalAccuracy: NaN,
                     coordinate: { latitude: NaN, longitude: NaN },
                     timestamp: new Date(NaN) }
    }
    function fail(code) { sourceError = code }
}
