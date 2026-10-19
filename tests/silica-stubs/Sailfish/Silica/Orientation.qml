import QtQuick 2.0

// Silica enum via QML `enum` (see PageStatus): page turn mask, Silica's values.
QtObject {
    enum Orientation {
        Portrait = 1,
        Landscape = 2,
        PortraitInverted = 4,
        PortraitMask = 5,
        LandscapeInverted = 8,
        LandscapeMask = 10,
        All = 15
    }
}
