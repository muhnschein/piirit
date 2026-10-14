import QtQuick 2.0

// Where MediaPlayer or Camera picture goes. Draws nothing: test checks `source`
// wiring.
Item {
    property var source
    property var fillMode
    property bool autoOrientation: false
    // Source frame, as phone back camera: landscape 4:3.
    property rect sourceRect: Qt.rect(0, 0, 1280, 960)
    // Point in source as fraction of size.
    function mapPointToSourceNormalized(point) {
        if (width <= 0 || height <= 0) { return Qt.point(-1, -1) }
        return Qt.point(point.x / width, point.y / height)
    }
}
