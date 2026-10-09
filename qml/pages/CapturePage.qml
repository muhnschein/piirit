import QtQuick 2.0
import QtMultimedia 5.6
import QtSensors 5.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../components"
import "../js/Viewfinder.js" as Viewfinder

/*
 * The camera, for a picture or a video to send.
 *
 * A page of its own, pushed by URL and connected to, the way the picker
 * pages are: it names a Camera, so a device without one costs this
 * button rather than the conversation, and it reports what was made on
 * `picked` and lets the caller decide what that means -- the same shape
 * as AttachPhotoPage. What is made goes to the captures directory
 * (Captures, capture.rs), where it waits to be sent; the core copies a
 * sent file into its own directory, and the page that sent it discards
 * the capture afterwards.
 *
 * Laid out, and working, as the platform's own camera (jolla-camera) in
 * portrait: the sensor's frame on black at the full width of the screen,
 * pushed down below the notch on a tall phone, and the controls over and
 * under it.
 *  - Along the top, what is in force -- flash, self-timer, white balance,
 *    grid -- and a tap there pulls the settings down (CameraSettingsPanel).
 *  - On the picture: pinch to zoom, with the platform's zoom line while it
 *    moves; tap to focus there, with a ring that lights once the lens has
 *    found it; the rule-of-thirds grid when it is on; and the exposure
 *    slider standing at the right edge.
 *  - At the foot: the two modes stacked on the left, the current one lit;
 *    the shutter in the middle, counting down when the self-timer is set;
 *    the other side's camera on the right, and above the shutter one ring
 *    per back camera on a phone with more than one.
 * While a video records only the shutter and the zoom are left, as in the
 * platform's camera, and the time runs at the top.
 *
 * The sensor's frame is the phone's landscape whichever way the phone
 * is held. The platform's camera writes the turn into the file rather
 * than turning the pixels -- a still's EXIF orientation, a video's
 * rotation -- from the orientation sensor, and so does this page.
 *
 * A still is written where it is asked to go. A video is written where
 * the recorder decides to put it, and is not done when it is stopped:
 * the recorder finalises the file first, and only then is it reported.
 * The recorder's own state is what the page draws from -- a tap on
 * record that the pipeline did not take leaves nothing half-armed --
 * and a stop the recorder ignores is followed by stopping the camera,
 * which finishes the file the way leaving the page does.
 *
 * The camera runs only while this page is the one on screen, and the
 * page takes itself down once it has something to report: a capture is
 * the answer to the question the page asks.
 */
Page {
    id: page

    /// The absolute path of the picture or video made.
    signal picked(string path)

    /// 0 for a still, 1 for a video.
    property int mode: 0
    /// A video is being recorded: the recorder's word, not the page's.
    readonly property bool recording:
        camera.videoRecorder.recorderState === page.recordingState
    /// A recording was asked for and no file has been reported yet.
    property bool videoWanted: false
    /// The recorder has been asked to stop, and the file is on its way.
    property bool stopping: false
    /// Something has been made and reported; nothing more is taken.
    property bool done: false
    /// Seconds recorded so far, counted here: the recorder's own
    /// duration stayed at nothing on a device.
    property int seconds: 0
    property string errorMessage: ""

    /// `CameraRecorder.RecordingState` and `CameraRecorder.FinalizingStatus`,
    /// written as their values: the enums live on the type, and the
    /// headless stub that stands in for QtMultimedia cannot declare one
    /// on Qt 5.6. See AttachmentPreview for the same problem.
    readonly property int recordingState: 1
    readonly property int recordingStatus: 5
    readonly property int finalizingStatus: 7

    /// The cameras on each side, as device ids, in the platform's order.
    readonly property var backCameras:
        Viewfinder.camerasFacing(QtMultimedia.availableCameras, Viewfinder.backFace)
    readonly property var frontCameras:
        Viewfinder.camerasFacing(QtMultimedia.availableCameras, Viewfinder.frontFace)
    /// Which side the camera in use faces.
    readonly property int facing: page.frontCameras.indexOf(camera.deviceId) >= 0
                                  ? Viewfinder.frontFace : Viewfinder.backFace
    /// The back camera last used, for the way back from the front.
    property string lastBackCamera: ""
    /// Where the switch button goes; empty on a phone with one side.
    readonly property string otherCamera:
        Viewfinder.otherSide(QtMultimedia.availableCameras, page.facing, page.lastBackCamera)

    /// The torch, in video. Not kept: the platform camera starts with it
    /// off too, and a light left on from last time is a surprise.
    property bool torch: false
    /// The white balance and exposure compensation, for this page only.
    property int whiteBalance: Viewfinder.whiteBalanceAuto
    property real exposure: 0

    /// What the flash panel offers, and what it is set to: the setting
    /// kept for a picture, the torch for a video.
    readonly property var flashModes: Viewfinder.flashModes(page.mode === 1, page.facing)
    readonly property int flash: page.mode === 1
                                 ? (page.torch ? Viewfinder.flashTorch : Viewfinder.flashOff)
                                 : Viewfinder.effectiveFlash(Settings.cameraFlash, false, page.facing)

    /// The self-timer: seconds still to go, while it runs.
    property real countdown: 0
    readonly property bool counting: selfTimer.running

    /// The shutter's face: the platform's own, a ring for a picture, a
    /// red dot to start a video and a square to stop it.
    readonly property string shutterIcon:
        page.mode === 0 ? "image://theme/icon-camera-shutter"
                        : page.recording ? "image://theme/icon-camera-video-shutter-off"
                                         : "image://theme/icon-camera-video-shutter-on"

    /// How far the phone is turned from upright, clockwise: 0, 90, 180
    /// or 270. From the orientation sensor, which the platform's camera
    /// reads too; the page itself stays put, as that camera does.
    property int pictureRotation: 0

    OrientationSensor {
        id: orientationSensor
        objectName: "orientationSensor"
        active: page.status === PageStatus.Active && !page.done
        onReadingChanged: page.turnWith(orientationSensor.reading
                                        ? orientationSensor.reading.orientation : 0)
    }

    /// `OrientationReading`'s values, written as such: TopUp 1, TopDown
    /// 2, LeftUp 3, RightUp 4. Face up, face down and unknown keep the
    /// last turn, which is what the platform's camera does too.
    function turnWith(orientation) {
        switch (orientation) {
        case 1: page.pictureRotation = 0; break
        case 2: page.pictureRotation = 180; break
        case 3: page.pictureRotation = 270; break
        case 4: page.pictureRotation = 90; break
        }
    }

    /// Where the next capture goes.
    Captures {
        id: captures
        objectName: "captures"
        onError: page.errorMessage = message
    }

    Camera {
        id: camera
        objectName: "camera"
        // Set by `setMode`, with the camera stopped, rather than bound:
        // the pipeline is rebuilt for the other mode, and a recording
        // asked for while that was under way was taken and not reported.
        captureMode: Camera.CaptureStillImage
        // The turn written into the file: the sensor's own mounting plus
        // the phone's, the front camera the other way round, which is the
        // sum the platform's camera apps write.
        metaData.orientation: page.facing === Viewfinder.frontFace
                              ? (720 + camera.orientation - page.pictureRotation) % 360
                              : (720 + camera.orientation + page.pictureRotation) % 360
        // Continuous, except for the few seconds after a tap: plain
        // autofocus on the point tapped, which is how the platform's
        // camera focuses where it is told to.
        focus {
            focusMode: focusHold.running ? Camera.FocusAuto : Camera.FocusContinuous
            focusPointMode: focusHold.running ? Camera.FocusPointCustom : Camera.FocusPointAuto
        }
        flash.mode: page.flash
        exposure.exposureCompensation: page.exposure
        imageProcessing.whiteBalanceMode: page.whiteBalance

        imageCapture {
            onImageSaved: page.report(path)
            onCaptureFailed: page.errorMessage = message
        }

        videoRecorder {
            // How much a minute of video costs, from the reader's
            // outgoing media quality setting. The bitrate and nothing
            // else: it is what decides the size of the file, and it
            // applies whatever the pipeline chose to record at, whereas
            // asking for a resolution this phone's encoder does not offer
            // is a recording that fails rather than a smaller one. The
            // two rates are deltachat-android's ceiling for a recoded
            // video and roughly what deltachat-ios's low preset comes to.
            // Nothing recodes a video here afterwards, so this is the one
            // chance to keep one inside what a relay takes.
            videoBitRate: Settings.mediaQuality === 1 ? 500000 : 1500000
            audioBitRate: Settings.mediaQuality === 1 ? 24000 : 64000
            // The status runs Recording, Finalizing, Loaded; the first
            // Loaded after a stop was asked for is the file. The location
            // is set as recording starts in the platform's backend, so it
            // alone says nothing.
            onRecorderStatusChanged: page.checkVideo()
            onRecorderStateChanged: page.checkVideo()
            onActualLocationChanged: page.checkVideo()
            // A record the pipeline would not take says why here, and
            // nowhere else: the state simply stays stopped.
            onError: page.errorMessage = errorString
        }
    }

    /// Switch between a still and a video. Done with the camera stopped,
    /// which is how the platform's own camera does it.
    function setMode(index) {
        if (index === page.mode || page.recording || page.stopping || page.done) {
            return
        }
        selfTimer.stop()
        camera.stop()
        page.mode = index
        camera.captureMode = index === 0 ? Camera.CaptureStillImage
                                         : Camera.CaptureVideo
        if (page.status === PageStatus.Active) {
            camera.start()
        }
    }

    /// Change to another camera: the other side's, or another lens on the
    /// back. The zoom and a tap's focus belong to the camera they were
    /// set on, so both start over, as they do in the platform's camera.
    function useCamera(deviceId) {
        if (deviceId.length === 0 || deviceId === camera.deviceId
                || page.recording || page.stopping || page.done) {
            return
        }
        selfTimer.stop()
        focusHold.stop()
        if (page.facing === Viewfinder.backFace && camera.deviceId.length > 0) {
            page.lastBackCamera = camera.deviceId
        }
        if (page.backCameras.indexOf(deviceId) >= 0) {
            page.lastBackCamera = deviceId
        }
        camera.deviceId = deviceId
        camera.digitalZoom = 1
    }

    /// Zoom to `value`, inside what the camera can do.
    function zoomTo(value) {
        camera.digitalZoom = Viewfinder.clampZoom(value, camera.maximumDigitalZoom)
        zoomIndicator.show()
    }

    /// Focus on a tap at `x`, `y` in the viewfinder. The ring goes where
    /// the finger was; the camera is told the point in its own frame.
    function focusAt(x, y) {
        var mapped = typeof viewfinder.mapPointToSourceNormalized === "function"
                ? viewfinder.mapPointToSourceNormalized(Qt.point(x, y)) : null
        var point = Viewfinder.focusPoint(mapped, x, y, viewfinder.width, viewfinder.height)
        if (!point) {
            return
        }
        reticle.at = Qt.point(x, y)
        focusHold.restart()
        camera.unlock()
        camera.focus.customFocusPoint = Qt.point(point.x, point.y)
        camera.searchAndLock()
    }

    // How long a tap's focus is kept: the platform camera's five seconds.
    // Then the lock comes off and continuous focus takes the lens back.
    Timer {
        id: focusHold
        objectName: "focusHold"
        interval: 5000
        onTriggered: camera.unlock()
    }

    /// The shutter: a picture, or the start or end of a video -- after
    /// the self-timer, when it is set. A tap while it counts calls it off.
    function shutter() {
        if (page.done) {
            return
        }
        if (selfTimer.running) {
            selfTimer.stop()
            page.countdown = 0
            return
        }
        if (Settings.cameraTimer > 0 && !page.recording && !page.stopping) {
            page.countdown = Settings.cameraTimer
            selfTimer.restart()
            return
        }
        page.fire()
    }

    /// What the shutter does once any wait is over.
    function fire() {
        if (page.done) {
            return
        }
        if (page.mode === 0) {
            var path = captures.new_path("photo", "jpg")
            if (path.length > 0) {
                blink.restart()
                camera.imageCapture.captureToLocation(path)
            }
        } else if (page.recording) {
            page.stopRecording()
        } else if (!page.stopping) {
            var target = captures.new_path("video", "mp4")
            if (target.length > 0) {
                camera.videoRecorder.outputLocation = Qt.resolvedUrl("file://" + target)
                page.seconds = 0
                page.videoWanted = true
                camera.videoRecorder.record()
            }
        }
    }

    SequentialAnimation {
        id: selfTimer
        objectName: "selfTimer"
        NumberAnimation {
            target: page
            property: "countdown"
            to: 0
            duration: Math.max(0, page.countdown) * 1000
        }
        ScriptAction { script: page.fire() }
    }

    function stopRecording() {
        page.stopping = true
        camera.videoRecorder.stop()
        // A stop the recorder does not act on within a moment is made
        // good by stopping the camera, which finishes the file: that is
        // what leaving the page did, and it worked where the button did
        // not.
        stopFallback.restart()
    }

    Timer {
        id: stopFallback
        objectName: "stopFallback"
        interval: 2000
        onTriggered: {
            if (page.stopping && !page.done) {
                camera.stop()
                giveUp.restart()
            }
        }
    }

    // Nothing came of that either: say so, and let the reader try again
    // with the camera running.
    Timer {
        id: giveUp
        interval: 3000
        onTriggered: {
            if (page.stopping && !page.done) {
                page.stopping = false
                page.videoWanted = false
                page.errorMessage = qsTr("The video could not be saved")
                if (page.status === PageStatus.Active) {
                    camera.start()
                }
            }
        }
    }

    /// The recorder has moved: a video that has finished writing, after
    /// a stop was asked for, is the answer.
    function checkVideo() {
        if (!page.videoWanted || !page.stopping || page.done) {
            return
        }
        var recorder = camera.videoRecorder
        if (recorder.recorderState === page.recordingState
                || recorder.recorderStatus === page.recordingStatus
                || recorder.recorderStatus === page.finalizingStatus) {
            return
        }
        var where = "" + recorder.actualLocation
        if (where.length === 0) {
            return
        }
        page.report(where.indexOf("file://") === 0 ? where.substring(7) : where)
    }

    /// Hand the capture back and leave.
    function report(path) {
        if (page.done) {
            return
        }
        page.done = true
        stopFallback.stop()
        giveUp.stop()
        camera.stop()
        page.picked(decodeURIComponent(path))
        // The answer given, the page goes: Silica's own pickers do the
        // same. Left to the caller otherwise it would sit under the
        // conversation with the camera off.
        pageStack.pop()
    }

    // The camera runs only while this page is the one on screen.
    onStatusChanged: {
        if (page.status === PageStatus.Active && !page.done) {
            camera.start()
        } else if (page.status !== PageStatus.Active) {
            selfTimer.stop()
            if (page.recording) {
                camera.videoRecorder.stop()
            }
            camera.stop()
        }
    }
    Component.onCompleted: {
        if (page.status === PageStatus.Active) {
            camera.start()
        }
    }

    // The clock for a video, counted here.
    Timer {
        objectName: "stopwatch"
        interval: 1000
        repeat: true
        running: page.recording
        onTriggered: page.seconds += 1
    }

    /// m:ss from seconds.
    function clock(seconds) {
        var rest = seconds % 60
        return Math.floor(seconds / 60) + ":" + (rest < 10 ? "0" : "") + rest
    }

    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    /// How tall the sensor's frame is for its width, once the camera says;
    /// four by three, the platform camera's default, until it does. The
    /// long side over the short: the page is upright and the frame is
    /// drawn upright, whichever way round the source reports itself.
    readonly property real frameRatio: Viewfinder.frameRatio(viewfinder.sourceRect.width,
                                                             viewfinder.sourceRect.height)
    /// The notch, where the phone has one.
    readonly property real topInset: Screen.topCutout.height

    // The sensor's frame, the full width of the screen. On a phone tall
    // enough to have room under it, it is pushed down past the notch and
    // the settings row, as the platform camera does on such a phone, so
    // neither covers the top of the picture.
    Item {
        id: frame
        objectName: "frame"
        width: parent.width
        height: Math.min(parent.height, Math.round(width * page.frameRatio))
        y: Math.max(0, Math.min(parent.height - height,
                                parent.height / Math.max(1, parent.width) >= 2
                                ? page.topInset + Theme.itemSizeLarge : 0))

        VideoOutput {
            id: viewfinder
            objectName: "viewfinder"
            anchors.fill: parent
            source: camera
            fillMode: VideoOutput.PreserveAspectFit

            SequentialAnimation {
                id: blink
                PropertyAction { target: viewfinder; property: "opacity"; value: 0 }
                PauseAnimation { duration: 100 }
                NumberAnimation { target: viewfinder; property: "opacity"; to: 1; duration: 300 }
            }
        }

        ViewfinderGrid {
            objectName: "grid"
            anchors.fill: parent
            visible: Settings.cameraGrid === true
        }

        FocusReticle {
            id: reticle
            objectName: "reticle"
            visible: focusHold.running
            locked: camera.lockStatus === Viewfinder.lockLocked
        }

        // Pinch to zoom, and tap to focus on what is under the finger.
        PinchArea {
            objectName: "pinchArea"
            anchors.fill: parent
            enabled: !page.done
            onPinchUpdated: page.zoomTo(Viewfinder.pinchZoom(camera.digitalZoom,
                                                             camera.maximumDigitalZoom,
                                                             pinch.scale, pinch.previousScale))

            MouseArea {
                objectName: "focusTap"
                anchors.fill: parent
                onClicked: page.focusAt(mouse.x, mouse.y)
            }
        }
    }

    ZoomIndicator {
        id: zoomIndicator
        objectName: "zoomIndicator"
        anchors {
            horizontalCenter: parent.horizontalCenter
            top: frame.top
            topMargin: Theme.itemSizeSmall + Theme.paddingLarge
        }
        zoom: camera.digitalZoom
        maximumZoom: camera.maximumDigitalZoom
    }

    // The time, while a video runs: at the top, below the notch, where
    // the platform camera writes it.
    Rectangle {
        objectName: "recordingIndicator"
        anchors.horizontalCenter: parent.horizontalCenter
        y: page.topInset + Theme.paddingMedium
        width: indicator.width + 2 * Theme.paddingLarge
        height: indicator.height + 2 * Theme.paddingSmall
        radius: height / 2
        color: Qt.rgba(0, 0, 0, 0.6)
        visible: page.recording || page.stopping

        Row {
            id: indicator
            anchors.centerIn: parent
            spacing: Theme.paddingMedium

            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.paddingMedium
                height: width
                radius: width / 2
                color: Theme.errorColor
            }

            Label {
                objectName: "recordingTime"
                anchors.verticalCenter: parent.verticalCenter
                color: "white"
                font.pixelSize: Theme.fontSizeLarge
                text: page.clock(page.seconds)
            }
        }
    }

    // Exposure, at the right edge of the picture. Not while recording.
    ExposureSlider {
        objectName: "exposureSlider"
        anchors {
            right: parent.right
            verticalCenter: frame.verticalCenter
        }
        height: Theme.itemSizeSmall * 5
        visible: !page.recording && !page.stopping && !page.done
        value: page.exposure
        onMoved: page.exposure = value
    }

    // The self-timer, counting, large over the middle of the picture.
    Label {
        objectName: "countdown"
        anchors.centerIn: frame
        visible: page.counting
        color: "white"
        font.pixelSize: Theme.fontSizeHuge
        text: Viewfinder.countdownText(page.countdown)
    }

    // The controls, at the foot: in the black under the picture where
    // there is room, over its bottom edge where there is not.
    Item {
        id: controls
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        height: Math.max(modes.height + 2 * Theme.paddingLarge,
                         parent.height - frame.y - frame.height)

        // The two modes, stacked, the current one on a lit disc. Not
        // while a video runs: the pipeline it would switch is the one
        // recording.
        Column {
            id: modes
            objectName: "modeColumn"
            anchors {
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            spacing: Theme.paddingSmall
            enabled: !page.recording && !page.stopping
            opacity: enabled ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 200 } }

            Repeater {
                model: ["image://theme/icon-camera-camera-mode", "image://theme/icon-camera-video"]

                Item {
                    width: Theme.itemSizeSmall
                    height: width

                    Rectangle {
                        anchors.fill: parent
                        radius: width / 2
                        visible: page.mode === index
                        color: Theme.highlightBackgroundColor
                    }

                    IconButton {
                        objectName: "modeOption" + index
                        anchors.centerIn: parent
                        icon.source: modelData
                        onClicked: page.setMode(index)
                    }
                }
            }
        }

        // One ring per back camera, above the shutter, on a phone that
        // has more than one. Not on the front, and not while a video
        // runs.
        CameraLensToggle {
            objectName: "lensToggle"
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: shutter.top
                bottomMargin: Theme.paddingMedium
            }
            cameras: page.backCameras
            current: camera.deviceId
            visible: page.backCameras.length > 1 && page.facing === Viewfinder.backFace
                     && !page.recording && !page.stopping
            onSelected: page.useCamera(deviceId)
        }

        // The shutter, in the platform's own drawing; the seconds left
        // on it while the self-timer runs.
        IconButton {
            id: shutter
            objectName: "shutter"
            anchors.centerIn: parent
            enabled: !page.done
            icon.source: page.shutterIcon
            onClicked: page.shutter()

            Label {
                anchors.centerIn: parent
                visible: Settings.cameraTimer > 0 && !page.recording && !page.counting
                color: "black"
                font.pixelSize: Theme.fontSizeTiny
                text: Settings.cameraTimer
            }
        }

        // The other side's camera. Not while a video runs.
        IconButton {
            objectName: "flipButton"
            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            visible: page.otherCamera.length > 0
            enabled: !page.recording && !page.stopping
            opacity: enabled ? 1 : 0
            icon.source: "image://theme/icon-camera-switch"
            onClicked: page.useCamera(page.otherCamera)
        }
    }

    // The capture is being written: nothing to tap meanwhile.
    BusyIndicator {
        objectName: "writing"
        anchors.centerIn: frame
        running: page.done || page.stopping || camera.imageCapture.capturing
        size: BusyIndicatorSize.Large
    }

    // Last, so that when it is down it lies over everything else and a
    // tap outside it puts it away rather than reaching what is under it.
    CameraSettingsPanel {
        objectName: "settingsPanel"
        anchors.fill: parent
        topInset: page.topInset
        visible: !page.recording && !page.stopping && !page.done
        flashModes: page.flashModes
        flash: page.flash
        timer: Settings.cameraTimer
        whiteBalance: page.whiteBalance
        grid: Settings.cameraGrid === true
        onFlashChosen: {
            if (page.mode === 1) {
                page.torch = mode === Viewfinder.flashTorch
            } else {
                Settings.cameraFlash = mode
            }
        }
        onTimerChosen: Settings.cameraTimer = seconds
        onWhiteBalanceChosen: page.whiteBalance = mode
        onGridChosen: Settings.cameraGrid = on
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }
}
