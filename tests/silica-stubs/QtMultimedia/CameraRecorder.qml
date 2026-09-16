import QtQuick 2.0

// The camera's video recorder. `recorderState` and `recorderStatus`
// carry QMediaRecorder's own values -- Stopped 0 / Recording 1, and
// Loaded 3 / Recording 5 / Finalizing 7 -- because the real enums live
// on the type and a QML component cannot declare one on Qt 5.6. A test
// walks the recorder through them the way the backend does: the location
// is set as recording starts, a stop finalises first, and only then is
// the file done.
QtObject {
    property url outputLocation
    property url actualLocation
    property int recorderState: 0
    property int recorderStatus: 3
    property int duration: 0
    /// What the page asks the encoder for, from the reader's outgoing
    /// media quality setting. Nothing here encodes anything; they are
    /// here so the page can name them.
    property int videoBitRate: 0
    property int audioBitRate: 0
    /// Whether `stop` is acted on; a backend that ignores it is the case
    /// the page's fallback is for.
    property bool stopsOnRequest: true
    /// What the backend says when a record or stop goes wrong.
    signal error(int errorCode, string errorString)
    function record() {
        actualLocation = outputLocation
        recorderState = 1
        recorderStatus = 5
    }
    /// Stops, but does not finish: the file is not done until `finish`.
    /// The status moves first, as it does in the backend, where both are
    /// set before either signal goes out.
    function stop() {
        if (!stopsOnRequest) { return }
        recorderStatus = 7
        recorderState = 0
    }
    /// The backend has finished writing the file where it was asked to.
    function finish() {
        recorderState = 0
        recorderStatus = 3
    }
}
