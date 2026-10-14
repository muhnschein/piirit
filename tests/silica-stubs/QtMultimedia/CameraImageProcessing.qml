import QtQuick 2.0

// Own type so `imageProcessing.whiteBalanceMode` binds. Values:
// QCameraImageProcessing, Auto 0, Sunlight 2, Cloudy 3, Tungsten 5, Fluorescent
// 6.
QtObject {
    property var whiteBalanceMode: 0
}
