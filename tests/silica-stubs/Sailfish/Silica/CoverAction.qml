import QtQuick 2.0

// Silica's CoverAction: the picture the home screen draws for it, and
// the signal a tap on it sends.
QtObject {
    property url iconSource
    signal triggered()
}
