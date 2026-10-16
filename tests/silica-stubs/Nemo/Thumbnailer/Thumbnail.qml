import QtQuick 2.0

// Stub: platform thumbnailer. Generates nothing; holds preview properties,
// draws nothing. `FillMode`: QML `enum` (host Qt 5.15), so pages name values.
Item {
    enum FillMode { Stretch, PreserveAspectFit, PreserveAspectCrop }
    property url source
    property string mimeType
    property int fillMode: 2
    property size sourceSize
}
