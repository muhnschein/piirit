import QtQuick 2.0

// Where a MediaPlayer's or a Camera's picture goes. Nothing is drawn
// here: what a test can check is that the page wires a source to an
// output at all, which is what `source` holds. `fillMode` takes the
// enum the page names, which reads as undefined here.
Item {
    property var source
    property var fillMode
    property bool autoOrientation: false
    // The source's frame, as the phone's back camera gives it: landscape,
    // four by three. A real one is empty until the first frame arrives.
    property rect sourceRect: Qt.rect(0, 0, 1280, 960)
    // Where a point on the output is in the source, as a fraction of it.
    // Nothing is fitted or turned here, so it is the point over the size.
    function mapPointToSourceNormalized(point) {
        if (width <= 0 || height <= 0) { return Qt.point(-1, -1) }
        return Qt.point(point.x / width, point.y / height)
    }
}
