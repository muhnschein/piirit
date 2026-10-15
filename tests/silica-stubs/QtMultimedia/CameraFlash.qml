import QtQuick 2.0

// Own type so grouped `flash.mode` binds. Values: QCameraExposure::FlashMode,
// Auto 1, Off 2, On 4, Torch 32.
QtObject {
    property var mode: 2
}
