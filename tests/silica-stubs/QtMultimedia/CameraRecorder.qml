import QtQuick 2.0

// Recorder. `recorderState`/`recorderStatus` = QMediaRecorder values (Stopped
// 0, Recording 1; Loaded 3, Recording 5, Finalizing 7). Test walks states as
// backend.
QtObject {
    property url outputLocation
    property url actualLocation
    property int recorderState: 0
    property int recorderStatus: 3
    property int duration: 0
    /// Encoder settings from media quality. Names only.
    property int videoBitRate: 0
    property int audioBitRate: 0
    /// Whether `stop` acts; ignored: page's fallback.
    property bool stopsOnRequest: true
    /// Backend error on record or stop failure.
    signal error(int errorCode, string errorString)
    function record() {
        actualLocation = outputLocation
        recorderState = 1
        recorderStatus = 5
    }
    /// Stops, not finished: file done at `finish`.
    function stop() {
        if (!stopsOnRequest) { return }
        recorderStatus = 7
        recorderState = 0
    }
    /// Backend finished writing file.
    function finish() {
        recorderState = 0
        recorderStatus = 3
    }
}
