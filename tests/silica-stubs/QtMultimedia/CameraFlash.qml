import QtQuick 2.0

// The camera's flash, a type of its own so a page's grouped
// `flash.mode` binding resolves against a declared property. The mode
// is QCameraExposure::FlashMode's value: Auto 1, Off 2, On 4, Torch 32.
QtObject {
    property var mode: 2
}
