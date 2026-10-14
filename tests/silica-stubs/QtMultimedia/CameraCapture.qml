import QtQuick 2.0

// Still capture. Writes nothing: test calls `saved` with page's path, as real
// one does when file lands.
QtObject {
    property bool capturing: false
    property bool ready: true
    /// Last capture's requested path.
    property string requested: ""
    signal imageSaved(int requestId, string path)
    signal captureFailed(int requestId, string message)
    function captureToLocation(location) {
        requested = location
        return 1
    }
    function capture() { return 1 }
    /// Requested file written.
    function saved() { imageSaved(1, requested) }
}
