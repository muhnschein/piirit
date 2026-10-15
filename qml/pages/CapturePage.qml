import QtQuick 2.0
import QtMultimedia 5.6
import QtSensors 5.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../components"
import "../js/Viewfinder.js" as Viewfinder

/*
 * Camera for picture or video to send; own page like pickers (no camera costs
 * one button). Reports on `picked`; file in captures dir (capture.rs), sending
 * page discards it. Layout and file turn (EXIF, rotation) as jolla-camera;
 * page holds still while recording. UI follows recorder's state (refused
 * record leaves nothing half-armed).
 */
Page {
    id: page

    allowedOrientations: page.recording || page.stopping ? page.orientation
                                                         : Viewfinder.allOrientations

    /// Path of picture or video made.
    signal picked(string path)

    /// 0 still, 1 video: opening mode, then mode stack's.
    property int mode: 0
    /// Opens on front (selfie quick action, QuickActions.js). Read at creation.
    property bool front: false
    /// Closes after reporting. False if caller replaces this page: pop would take
    /// that too (see ChatPickerPage `closeOnPick`).
    property bool closeOnPick: true
    /// Cover quick action would pop recording; fine once reported.
    readonly property bool pausesQuickActions: (page.recording || page.stopping) && !page.done
    /// Video being recorded: recorder's word.
    readonly property bool recording:
        camera.videoRecorder.recorderState === page.recordingState
    /// Video recording or finishing: camera runs in background.
    readonly property bool filming: page.recording || page.stopping
    /// App in front. Writable: tests set it.
    property bool appActive: Qt.application.state === Qt.ApplicationActive
    /// Page on screen (Silica keeps it active in background).
    readonly property bool shown: page.status === PageStatus.Active && page.appActive
    /// Recording asked for; no file reported yet.
    property bool videoWanted: false
    /// Recorder asked to stop; file on its way.
    property bool stopping: false
    /// Made and reported; nothing more taken.
    property bool done: false
    /// Seconds counted here: recorder's duration reads 0 on device.
    property int seconds: 0
    property string errorMessage: ""

    /// Recorder enums as values: missing in Qt 5.6 headless stub. See
    /// AttachmentPreview.
    readonly property int recordingState: 1
    readonly property int recordingStatus: 5
    readonly property int finalizingStatus: 7

    /// Cameras per side, device ids, platform order.
    readonly property var backCameras:
        Viewfinder.camerasFacing(QtMultimedia.availableCameras, Viewfinder.backFace)
    readonly property var frontCameras:
        Viewfinder.camerasFacing(QtMultimedia.availableCameras, Viewfinder.frontFace)
    /// Side the camera in use faces.
    readonly property int facing: page.frontCameras.indexOf(camera.deviceId) >= 0
                                  ? Viewfinder.frontFace : Viewfinder.backFace
    /// Last back camera, for return from front.
    property string lastBackCamera: ""
    /// Switch-button target; empty on one-sided phone.
    readonly property string otherCamera:
        Viewfinder.otherSide(QtMultimedia.availableCameras, page.facing, page.lastBackCamera)

    /// Torch in video. Not kept: light left on surprises.
    property bool torch: false
    /// White balance and exposure compensation, this page only.
    property int whiteBalance: Viewfinder.whiteBalanceAuto
    property real exposure: 0

    /// Flash panel options and setting: picture's, or torch for video.
    readonly property var flashModes: Viewfinder.flashModes(page.mode === 1, page.facing)
    readonly property int flash: page.mode === 1
                                 ? (page.torch ? Viewfinder.flashTorch : Viewfinder.flashOff)
                                 : Viewfinder.effectiveFlash(Settings.cameraFlash, false, page.facing)

    /// Self-timer seconds left while running.
    property real countdown: 0
    readonly property bool counting: selfTimer.running

    /// Shutter: ring for picture; red dot starts video, square stops.
    readonly property string shutterIcon:
        page.mode === 0 ? "image://theme/icon-camera-shutter"
                        : page.recording ? "image://theme/icon-camera-video-shutter-off"
                                         : "image://theme/icon-camera-video-shutter-on"

    /// Phone turn clockwise: 0/90/180/270. Page stays put.
    property int pictureRotation: 0

    OrientationSensor {
        id: orientationSensor
        objectName: "orientationSensor"
        active: page.shown && !page.done
        onReadingChanged: page.turnWith(orientationSensor.reading
                                        ? orientationSensor.reading.orientation : 0)
    }

    /// `OrientationReading` values: TopUp 1, TopDown 2, LeftUp 3, RightUp 4.
    /// Face up/down/unknown keep last turn.
    function turnWith(orientation) {
        switch (orientation) {
        case 1: page.pictureRotation = 0; break
        case 2: page.pictureRotation = 180; break
        case 3: page.pictureRotation = 270; break
        case 4: page.pictureRotation = 90; break
        }
    }

    /// Where next capture goes.
    Captures {
        id: captures
        objectName: "captures"
        onError: page.errorMessage = message
    }

    Camera {
        id: camera
        objectName: "camera"
        // Set by `setMode` while stopped: recording asked mid-rebuild never
        // reported.
        captureMode: Camera.CaptureStillImage
        // File's turn: sensor mounting + phone's (front reversed).
        metaData.orientation: page.facing === Viewfinder.frontFace
                              ? (720 + camera.orientation - page.pictureRotation) % 360
                              : (720 + camera.orientation + page.pictureRotation) % 360
        // Continuous; tap focus for a few seconds.
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
            // Bitrate sets size, not resolution (encoder may refuse). Rates:
            // deltachat-android recode ceiling, ~deltachat-ios low preset. No
            // recode later: one chance to fit relay limit.
            videoBitRate: Settings.mediaQuality === 1 ? 500000 : 1500000
            audioBitRate: Settings.mediaQuality === 1 ? 24000 : 64000
            // First Loaded after stop = file. Location set at record start by
            // backend: says nothing alone.
            onRecorderStatusChanged: page.checkVideo()
            onRecorderStateChanged: page.checkVideo()
            onActualLocationChanged: page.checkVideo()
            // Pipeline refusal shown only here: state stays stopped.
            onError: page.errorMessage = errorString
        }
    }

    /// Still <-> video switch, camera stopped (as platform camera).
    function setMode(index) {
        if (index === page.mode || page.recording || page.stopping || page.done) {
            return
        }
        selfTimer.stop()
        camera.stop()
        page.mode = index
        camera.captureMode = index === 0 ? Camera.CaptureStillImage
                                         : Camera.CaptureVideo
        if (page.shown) {
            camera.start()
        }
    }

    /// Other side or back lens; zoom and tap focus reset.
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

    /// Zoom to `value`, within camera range.
    function zoomTo(value) {
        camera.digitalZoom = Viewfinder.clampZoom(value, camera.maximumDigitalZoom)
        zoomIndicator.show()
    }

    /// Tap focus at `x`, `y` in viewfinder; ring at finger.
    function focusAt(x, y) {
        // Tap in picture terms: viewfinder is turned.
        var local = viewfinder.mapFromItem(frame, x, y)
        var mapped = typeof viewfinder.mapPointToSourceNormalized === "function"
                ? viewfinder.mapPointToSourceNormalized(Qt.point(local.x, local.y)) : null
        var point = Viewfinder.focusPoint(mapped, local.x, local.y,
                                          viewfinder.width, viewfinder.height)
        if (!point) {
            return
        }
        reticle.at = Qt.point(x, y)
        focusHold.restart()
        camera.unlock()
        camera.focus.customFocusPoint = Qt.point(point.x, point.y)
        camera.searchAndLock()
    }

    // Tap focus kept 5 s (platform's), then continuous.
    Timer {
        id: focusHold
        objectName: "focusHold"
        interval: 5000
        onTriggered: camera.unlock()
    }

    /// Shutter: picture, or video start/end. After self-timer; tap while counting
    /// cancels.
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

    /// What shutter does once any wait is over.
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
        // Stop ignored by recorder: stopping camera finishes file (leaving page
        // worked where button didn't).
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

    // Nothing came: say so; reader retries, camera running.
    Timer {
        id: giveUp
        interval: 3000
        onTriggered: {
            if (page.stopping && !page.done) {
                page.stopping = false
                page.videoWanted = false
                page.errorMessage = qsTr("The video could not be saved")
                if (page.shown) {
                    camera.start()
                }
            }
        }
    }

    /// Recorder state: finished writing after stop = answer.
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

    /// Hand capture back and leave.
    function report(path) {
        if (page.done) {
            return
        }
        page.done = true
        stopFallback.stop()
        giveUp.stop()
        camera.stop()
        page.picked(decodeURIComponent(path))
        // Answered: page goes (as Silica pickers), else sits under conversation.
        if (page.closeOnPick) {
            pageStack.pop()
        }
    }

    // Camera only while page on screen. From status, not `shown`: handler can
    // run before binding.
    onStatusChanged: {
        if (page.status === PageStatus.Active && page.appActive && !page.done) {
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
        // Opening mode before start: setMode would rebuild.
        if (page.mode === 1) {
            camera.captureMode = Camera.CaptureVideo
        }
        if (page.front && page.frontCameras.length > 0) {
            page.useCamera(page.frontCameras[0])
        }
        if (page.shown) {
            camera.start()
        }
    }

    // Off in background or screen off, except video recording or finishing.
    onAppActiveChanged: page.followApp()
    // Recording ends in background: camera goes then.
    onFilmingChanged: {
        if (!page.filming && !page.appActive) {
            page.followApp()
        }
    }
    function followApp() {
        if (page.status !== PageStatus.Active || page.done || page.filming) {
            return
        }
        if (page.appActive) {
            camera.start()
        } else {
            selfTimer.stop()
            camera.stop()
        }
    }

    // Video clock, counted here.
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

    // Black behind all, also strip Silica leaves beside landscape notch.
    Rectangle {
        objectName: "backdrop"
        anchors {
            fill: parent
            leftMargin: -page.sideInset
            rightMargin: -page.sideInset
        }
        color: "black"
    }

    /// Wider than tall: frame left, controls right.
    readonly property bool landscape: page.width > page.height
    /// Silica's page turn, clockwise.
    readonly property int turn: Viewfinder.pageTurn(page.orientation)

    /// Frame height for width once camera says; default 4:3. Long over short:
    /// upright however source reports.
    readonly property real frameRatio: Viewfinder.frameRatio(viewfinder.sourceRect.width,
                                                             viewfinder.sourceRect.height)
    /// Notch: top or side (outside page; Silica clears it).
    readonly property real topInset: page.landscape ? 0 : Screen.topCutout.height
    readonly property real sideInset: page.landscape ? Screen.topCutout.height : 0

    // Frame full width, tall phones below notch and settings row; landscape:
    // full height.
    Item {
        id: frame
        objectName: "frame"
        width: page.landscape
               ? Math.min(parent.width, Math.round(parent.height * page.frameRatio))
               : parent.width
        height: page.landscape
                ? parent.height
                : Math.min(parent.height, Math.round(width * page.frameRatio))
        x: 0
        y: page.landscape
           ? 0
           : Math.max(0, Math.min(parent.height - height,
                                  parent.height / Math.max(1, parent.width) >= 2
                                  ? page.topInset + Theme.itemSizeLarge : 0))

        // Turned against page. Item's own turn, not VideoOutput `orientation`
        // (Qt 5.6 keeps pre-turn shape, squeezes).
        VideoOutput {
            id: viewfinder
            objectName: "viewfinder"
            readonly property bool across: page.turn % 180 !== 0
            anchors.centerIn: parent
            width: across ? parent.height : parent.width
            height: across ? parent.width : parent.height
            rotation: -page.turn
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

        // Pinch zoom; tap focus on finger point.
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
            horizontalCenter: frame.horizontalCenter
            top: frame.top
            topMargin: Theme.itemSizeSmall + Theme.paddingLarge
        }
        zoom: camera.digitalZoom
        maximumZoom: camera.maximumDigitalZoom
    }

    // Video time at top, below notch.
    Rectangle {
        objectName: "recordingIndicator"
        anchors.horizontalCenter: frame.horizontalCenter
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

    // Exposure at right edge (landscape: left, opposite shutter). Not recording.
    ExposureSlider {
        objectName: "exposureSlider"
        // By x, not swapped anchors: both set mid-turn stretched slider for good.
        x: page.landscape ? frame.x + Theme.paddingLarge : frame.x + frame.width - width
        anchors.verticalCenter: frame.verticalCenter
        labelOnLeft: !page.landscape
        height: Theme.itemSizeSmall * 5
        visible: !page.recording && !page.stopping && !page.done
        value: page.exposure
        onMoved: page.exposure = value
    }

    // Self-timer count, large, over picture middle.
    Label {
        objectName: "countdown"
        anchors.centerIn: frame
        visible: page.counting
        color: "white"
        font.pixelSize: Theme.fontSizeHuge
        text: Viewfinder.countdownText(page.countdown)
    }

    // Controls under picture if room, else over bottom edge; landscape: right.
    Item {
        id: controls
        objectName: "controls"
        readonly property real depth: page.landscape
            ? Math.max(modes.width + 2 * Theme.paddingLarge,
                       parent.width - frame.x - frame.width)
            : Math.max(modes.height + 2 * Theme.paddingLarge,
                       parent.height - frame.y - frame.height)
        x: page.landscape ? parent.width - width : 0
        y: page.landscape ? 0 : parent.height - height
        width: page.landscape ? depth : parent.width
        height: page.landscape ? parent.height : depth

        // Two modes stacked, current lit. Not while recording (would switch
        // pipeline).
        Column {
            id: modes
            objectName: "modeColumn"
            x: page.landscape ? (parent.width - width) / 2 : Theme.horizontalPageMargin
            y: page.landscape ? Theme.horizontalPageMargin : (parent.height - height) / 2
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

        // Back-lens rings above shutter; not front or recording.
        CameraLensToggle {
            objectName: "lensToggle"
            // Two paddings clear of shutter in landscape.
            x: page.landscape ? shutter.x - width - 2 * Theme.paddingLarge
                              : (parent.width - width) / 2
            y: page.landscape ? (parent.height - height) / 2
                              : shutter.y - height - Theme.paddingMedium
            vertical: page.landscape
            cameras: page.backCameras
            labels: Settings.backLensLabels
            current: camera.deviceId
            visible: page.backCameras.length > 1 && page.facing === Viewfinder.backFace
                     && !page.recording && !page.stopping
            onSelected: page.useCamera(deviceId)
        }

        // Platform shutter; self-timer seconds on it.
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

        // Other side. Not recording.
        IconButton {
            objectName: "flipButton"
            x: page.landscape ? (parent.width - width) / 2
                              : parent.width - width - Theme.horizontalPageMargin
            y: page.landscape ? parent.height - height - Theme.horizontalPageMargin
                              : (parent.height - height) / 2
            visible: page.otherCamera.length > 0
            enabled: !page.recording && !page.stopping
            opacity: enabled ? 1 : 0
            icon.source: "image://theme/icon-camera-switch"
            onClicked: page.useCamera(page.otherCamera)
        }
    }

    // Capture being written: nothing to tap.
    BusyIndicator {
        objectName: "writing"
        anchors.centerIn: frame
        running: page.done || page.stopping || camera.imageCapture.capturing
        size: BusyIndicatorSize.Large
    }

    // Last: over all; tap outside closes.
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
