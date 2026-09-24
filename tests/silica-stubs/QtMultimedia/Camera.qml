import QtQuick 2.0

// The camera. Nothing is captured here; what a test can check is that a
// page starts and stops it with its own status, wires it to a viewfinder,
// asks it to focus, and drives its still and video capture through the
// two objects below. The enums the pages name (Camera.FocusContinuous
// and the like) are the real type's and read as undefined here.
QtObject {
    property bool running: false
    property var captureMode
    // QCamera::Position's own values: BackFace 1, FrontFace 2. The enum
    // reads as undefined here, and a page comparing against it must not
    // be handed undefined on this side too.
    property var position: 1
    // The sensor's mounting, in degrees, as a phone's back camera reports
    // it: its frame is the phone's landscape.
    property int orientation: 90
    property CameraFocus focus: CameraFocus {}
    property CameraMetaData metaData: CameraMetaData {}
    property CameraCapture imageCapture: CameraCapture {}
    property CameraRecorder videoRecorder: CameraRecorder {}
    /// How many focus searches were asked for.
    property int searches: 0
    // What the camera can run its viewfinder at, and what it was asked
    // for. A real camera answers this only once it is loaded, and some
    // answer with nothing at all, so a test can empty the list to check
    // that the view carries on regardless.
    property var viewfinderResolutions: [
        Qt.size(640, 480),
        Qt.size(1280, 960),
        Qt.size(1920, 1440)
    ]
    property CameraViewfinder viewfinder: CameraViewfinder {}
    // QCamera::Status values: UnloadedStatus 1, LoadedStatus 3.
    property int cameraStatus: 3
    function supportedViewfinderResolutions() { return viewfinderResolutions }
    function start() { running = true }
    // Stopping the camera stops a recording with it, finished or not: what
    // the page falls back on when the recorder ignores its own stop.
    function stop() {
        running = false
        if (videoRecorder.recorderState === 1) {
            videoRecorder.recorderState = 0
            videoRecorder.recorderStatus = 3
        }
    }
    function searchAndLock() { searches += 1 }
    function unlock() { }
}
