import QtQuick 2.0

// The camera's image processing, a type of its own so a page's grouped
// `imageProcessing.whiteBalanceMode` binding resolves against a declared
// property. The mode is QCameraImageProcessing's value: Auto 0,
// Sunlight 2, Cloudy 3, Tungsten 5, Fluorescent 6.
QtObject {
    property var whiteBalanceMode: 0
}
