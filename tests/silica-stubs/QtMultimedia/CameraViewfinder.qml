import QtQuick 2.0

// Own type so `camera.viewfinder.resolution` assigns as real type. Renders
// nothing; test reads what page asked.
QtObject {
    property size resolution
}
