import QtQuick 2.0

// Platform's share sheet for what app hands it. Opens nothing: counts triggers,
// keeps what was handed, for tests to read.
Item {
    property string mimeType
    property var resources: []
    property string title
    /// Sheets asked for.
    property int triggered: 0

    function trigger() {
        triggered = triggered + 1
    }
}
