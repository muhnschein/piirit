import QtQuick 2.0

// The camera's viewfinder settings, a type of its own so a page can
// assign into `camera.viewfinder.resolution` the way the real type
// allows. Nothing renders here; what a test reads back is what the page
// asked the camera for.
QtObject {
    property size resolution
}
