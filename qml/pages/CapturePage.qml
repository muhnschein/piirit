import QtQuick 2.0
import QtMultimedia 5.6
import QtSensors 5.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../components"

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
 * Laid out as the platform's own camera lays itself out: the viewfinder
 * on black, as large as the sensor's frame lets it be above the
 * controls, and the controls in the black below it -- the two modes
 * stacked on the left with the current one lit, the shutter in the
 * middle, the other camera on the right. The time sits over the bottom
 * of the viewfinder, above the shutter, while a video records: the top
 * of the screen is under a notch on some phones.
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
        metaData.orientation: camera.position === Camera.FrontFace
                              ? (720 + camera.orientation - page.pictureRotation) % 360
                              : (720 + camera.orientation + page.pictureRotation) % 360
        focus {
            focusMode: Camera.FocusContinuous
            focusPointMode: Camera.FocusPointAuto
        }

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
        camera.stop()
        page.mode = index
        camera.captureMode = index === 0 ? Camera.CaptureStillImage
                                         : Camera.CaptureVideo
        if (page.status === PageStatus.Active) {
            camera.start()
        }
    }

    /// A picture, or the start or end of a video.
    function shutter() {
        if (page.done) {
            return
        }
        if (page.mode === 0) {
            var path = captures.new_path("photo", "jpg")
            if (path.length > 0) {
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

    // The sensor's frame, on black, as large as the room above the
    // controls lets it be.
    VideoOutput {
        id: viewfinder
        objectName: "viewfinder"
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: controls.top
        }
        source: camera
        fillMode: VideoOutput.PreserveAspectFit

        // Tap to focus on what is under the finger, as the scanner does.
        MouseArea {
            objectName: "focusTap"
            anchors.fill: parent
            onClicked: {
                camera.focus.focusPointMode = Camera.FocusPointCustom
                camera.focus.customFocusPoint = Qt.point(mouse.x / width, mouse.y / height)
                camera.unlock()
                camera.searchAndLock()
            }
        }
    }

    // The time, while a video runs: a pill over the bottom of the
    // viewfinder, centred above the shutter.
    Rectangle {
        objectName: "recordingIndicator"
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: controls.top
            bottomMargin: Theme.paddingMedium
        }
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
                font.pixelSize: Theme.fontSizeMedium
                text: page.clock(page.seconds)
            }
        }
    }

    // The controls, in the black under the viewfinder.
    Item {
        id: controls
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        // Tall enough for the two stacked modes with a margin, which
        // the shutter's own size is not: the lower mode sat on the
        // screen's edge.
        height: modes.height + 2 * Theme.paddingLarge

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

        // The shutter, in the platform's own drawing.
        IconButton {
            id: shutter
            objectName: "shutter"
            anchors.centerIn: parent
            enabled: !page.done
            icon.source: page.shutterIcon
            onClicked: page.shutter()
        }

        // The other camera: the one facing the reader. Not while a video
        // runs.
        IconButton {
            objectName: "flipButton"
            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            enabled: !page.recording && !page.stopping
            icon.source: "image://theme/icon-camera-switch"
            onClicked: camera.position = camera.position === Camera.FrontFace
                                         ? Camera.BackFace : Camera.FrontFace
        }
    }

    // The capture is being written: nothing to tap meanwhile.
    BusyIndicator {
        objectName: "writing"
        anchors.centerIn: viewfinder
        running: page.done || page.stopping || camera.imageCapture.capturing
        size: BusyIndicatorSize.Large
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
