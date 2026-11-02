import QtQuick 2.0

// Stub: captures nothing. Test checks start/stop, viewfinder, focus, capture.
// Enums = QCamera values; other names (Camera.FrontFace...) undefined.
QtObject {
    enum CaptureMode { CaptureViewfinder = 0, CaptureStillImage = 1, CaptureVideo = 2 }
    enum FocusMode {
        FocusManual = 1, FocusHyperfocal = 2, FocusInfinity = 4,
        FocusAuto = 8, FocusContinuous = 16, FocusMacro = 32
    }
    enum FocusPointMode {
        FocusPointAuto = 0, FocusPointCenter = 1,
        FocusPointFaceDetection = 2, FocusPointCustom = 3
    }
    // QCamera::State. Real one built Active: starts itself unless page says Unloaded.
    enum State { UnloadedState = 0, LoadedState = 1, ActiveState = 2 }
    property int cameraState: 2
    readonly property bool running: cameraState === 2
    /// Device and mode at each `start()` that started it, "device/mode", '|' between.
    /// Qt's start on build not listed: shows as `running`.
    property string starts: ""
    property var captureMode
    // QCamera::Position: BackFace 1, FrontFace 2. Keep `position` a number.
    property var position: 1
    // Sensor mount, degrees: landscape.
    property int orientation: 90
    // Default id until page picks one.
    property string deviceId: "back-0"
    // Digital zoom x4. Real one clamps; test checks page never asks past max.
    property real digitalZoom: 1
    property real maximumDigitalZoom: 4
    // QCamera::LockStatus: Unlocked 0, Searching 1, Locked 2. Test sets: focus
    // found.
    property int lockStatus: 0
    property CameraFocus focus: CameraFocus {}
    property CameraFlash flash: CameraFlash {}
    property CameraExposure exposure: CameraExposure {}
    property CameraImageProcessing imageProcessing: CameraImageProcessing {}
    property CameraMetaData metaData: CameraMetaData {}
    property CameraCapture imageCapture: CameraCapture {}
    property CameraRecorder videoRecorder: CameraRecorder {}
    /// Focus searches and lock releases asked for.
    property int searches: 0
    property int unlocks: 0
    // Viewfinder resolutions, asked for. Test empties list: view must carry on.
    property var viewfinderResolutions: [
        Qt.size(640, 480),
        Qt.size(1280, 960),
        Qt.size(1920, 1440)
    ]
    property CameraViewfinder viewfinder: CameraViewfinder {}
    // QCamera::Status: UnloadedStatus 1, LoadedStatus 3.
    property int cameraStatus: 3
    function supportedViewfinderResolutions() { return viewfinderResolutions }
    function start() {
        if (cameraState !== 2) {
            cameraState = 2
            starts += (starts.length > 0 ? "|" : "") + deviceId + "/" + captureMode
        }
    }
    // Stop also ends recording: page's fallback.
    function stop() {
        cameraState = 1
        if (videoRecorder.recorderState === 1) {
            videoRecorder.recorderState = 0
            videoRecorder.recorderStatus = 3
        }
    }
    function searchAndLock() { searches += 1 }
    function unlock() { unlocks += 1 }
}
