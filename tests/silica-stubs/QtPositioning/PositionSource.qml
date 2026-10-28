import QtQuick 2.0

// Stub: phone's position. Finds nothing; test hands fix via `report`, loses it via `forget`,
// failure via `fail` (Qt's `sourceError` numbers: 0 none, 1 access, 2 closed). New object =
// new fix. Unknown accuracy NaN, as Qt.
QtObject {
    property bool active: false
    property int updateInterval: 0
    property bool valid: true
    property int sourceError: 0
    property var position: ({ latitudeValid: false, longitudeValid: false,
                              horizontalAccuracyValid: false, horizontalAccuracy: NaN,
                              coordinate: { latitude: 0, longitude: 0 } })
    function report(latitude, longitude, accuracy) {
        position = { latitudeValid: true, longitudeValid: true,
                     horizontalAccuracyValid: accuracy > 0,
                     horizontalAccuracy: accuracy > 0 ? accuracy : NaN,
                     coordinate: { latitude: latitude, longitude: longitude } }
    }
    function forget() {
        position = { latitudeValid: false, longitudeValid: false,
                     horizontalAccuracyValid: false, horizontalAccuracy: NaN,
                     coordinate: { latitude: NaN, longitude: NaN } }
    }
    function fail(code) { sourceError = code }
}
