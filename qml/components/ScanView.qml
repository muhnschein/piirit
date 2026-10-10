import QtQuick 2.0
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import Piirit 1.0
import "."
import "../js/Viewfinder.js" as Viewfinder

/*
 * Point the camera at a QR code -- or, from the button under the
 * viewfinder, type or paste the link the code would carry. What either
 * says is handed out on `scanned` and the view waits; what to do with it
 * is the host's -- an invite is followed -- and the core is asked what
 * it is first, the same as for a pasted link.
 *
 * A component of its own, loaded by URL by the pages that want it -- the
 * QR page for an invite, the take-over page for the code a device shows
 * while offering its profile: this is the only file that names a Camera,
 * so a device without one costs the scanner rather than the page around
 * it. Both give it the rest of the page, which is what a code needs to
 * land in enough pixels to read.
 *
 * What it is looking for is the host's to say (`hintText`), and so is
 * whether the link behind the code can be typed instead (`offerLink`).
 *
 * Frames come through `grabToImage`, which is the one way QML on Qt 5.6
 * hands a viewfinder's pixels to anything, and go to the scanner as a
 * file. The next frame is grabbed only once the last has been decoded, so
 * a slow decode never queues frames behind it.
 *
 * Once a code is read the view stays, with the camera off and a busy
 * indicator on, until the host takes it down: Silica drops any stack
 * operation asked for while a transition is running, so a page that
 * popped itself here would have the host's own navigation -- opening the
 * chat the invite led to -- land during the pop and be lost. The typed
 * link is a panel over the viewfinder rather than a dialog for the same
 * reason: a dialog's own pop would be the transition the host's
 * navigation lands in.
 *
 * Focus is the camera's own: continuous autofocus, left to run, and a
 * tap on the viewfinder for a reader who wants the point under the
 * finger. Nothing else asks for a focus search, because in the video
 * pipeline a search does not search. The platform's camera stack turns
 * continuous focus into the Android HAL's "continuous-video" mode when
 * the camera captures video, and the HAL's own state machine says that a
 * focus trigger in that mode locks the lens where it is, at once,
 * mid-sweep or not -- unlike "continuous-picture", which finishes the
 * sweep first. A search every couple of seconds, which this view used to
 * run, therefore pinned the lens wherever it happened to be, from the
 * moment the camera started, and gave continuous focus no time between
 * the unlock and the next lock to find the code: the likeliest reason a
 * Jolla Phone's viewfinder stayed blurred until the system camera app,
 * opened alongside, took the lens over and focused it.
 * CodeReader and Foil Auth, which read codes on the same platform, run
 * continuous focus the same way and only search on a tap.
 *
 * A code that is small or far off is brought closer the platform
 * camera's way, with a pinch and its zoom line, or a thumb's: a button
 * steps through one, two and four times. Over the bottom of the picture,
 * as in the platform camera, are the torch -- a code on paper in a dim
 * room is otherwise unreadable -- one ring per back camera on a phone with
 * more than one, and the switch to the other side, for a code shown on a
 * screen held up to the front.
 */
Item {
    id: root

    /// The view is the one on screen. The camera runs only then.
    property bool active: false

    /// How the host page is turned, as Silica's `Orientation`. The
    /// camera's frame is fixed to the phone, so the picture is turned
    /// back by as much as the page is: upright on a page that turns with
    /// the phone.
    property int pageOrientation: Viewfinder.portrait

    /// A code was read, or a link typed, and this is its text.
    signal scanned(string text)
    /// The scanner could not read a frame at all. The message is its own.
    signal failed(string message)

    /// Set once a code is read, so a second frame in flight cannot
    /// report the same code twice.
    property bool done: false
    /// The link panel is open. The camera keeps running behind it: a
    /// reader who opened the panel and then found the code can still
    /// hold the phone up to it.
    property bool typing: false

    /// How far below the top of the view the line sits, for a host that
    /// has its own words up there already.
    property real hintTopMargin: 0

    /// The line saying what to point the camera at. The
    /// host's words, because what a code means is the host's: an invite
    /// on the QR page, the other phone's offer of a profile on the
    /// take-over page.
    property string hintText: qsTr("Point the camera at someone's invite code")

    /// Whether what the code carries can be typed or pasted instead.
    /// Worth having wherever the reader can get at the text another way,
    /// which is every camera in this app: a camera that will not read is
    /// otherwise a dead end.
    property bool offerLink: true

    /// What the typed half is called, since what the code carries
    /// differs by host: an invite someone sent, or the string a device
    /// shows beside the code it is offering its profile behind.
    property string linkButtonText: qsTr("Enter invite link")
    property string linkLabel: qsTr("Invite link")
    property string linkPlaceholder: "https://i.delta.chat/..."
    property string linkActionText: qsTr("Connect")

    /// The starts of the things this host's code carries, lower case.
    /// Only used to decide whether the clipboard is worth pasting into
    /// the field; what the payload means is the core's call.
    property var linkPrefixes: [
        "https://i.delta.chat/",
        "openpgp4fpr:",
        "dcaccount:",
        "dclogin:"
    ]

    /// What was read or typed, once it is worth acting on.
    function found(text) {
        if (root.done) {
            return
        }
        root.done = true
        camera.stop()
        root.scanned(text)
    }

    /// Open the link panel, with the clipboard's contents in it when
    /// they look like an invite: pasting is what the panel is for, and a
    /// link copied a moment ago is the likely reason it was opened.
    function typeLink() {
        root.typing = true
        var clip = Clipboard.text
        if (linkField.text.length === 0 && clip.length > 0 && root.looksLikeInvite(clip)) {
            linkField.text = clip
        }
    }

    /// Whether some text is one of the things this host's code carries,
    /// so the clipboard is not pasted into the field when it holds a
    /// shopping list.
    function looksLikeInvite(text) {
        var trimmed = text.trim().toLowerCase()
        for (var i = 0; i < root.linkPrefixes.length; i++) {
            if (trimmed.indexOf(root.linkPrefixes[i].toLowerCase()) === 0) {
                return true
            }
        }
        return false
    }

    function useLink() {
        var text = linkField.text.trim()
        if (text.length === 0) {
            return
        }
        root.found(text)
    }

    QrScanner {
        id: scanner
        objectName: "scanner"
        onFound: root.found(text)
        onError: root.failed(message)
    }

    /// The cameras on each side, as device ids, in the platform's order.
    readonly property var backCameras:
        Viewfinder.camerasFacing(QtMultimedia.availableCameras, Viewfinder.backFace)
    readonly property var frontCameras:
        Viewfinder.camerasFacing(QtMultimedia.availableCameras, Viewfinder.frontFace)
    readonly property int facing: root.frontCameras.indexOf(camera.deviceId) >= 0
                                  ? Viewfinder.frontFace : Viewfinder.backFace
    property string lastBackCamera: ""
    readonly property string otherCamera:
        Viewfinder.otherSide(QtMultimedia.availableCameras, root.facing, root.lastBackCamera)

    /// The torch is on. Only on the back, which is where the light is.
    property bool torch: false

    /// Change to another camera. The zoom and a tap's focus belong to the
    /// one they were set on, and the torch to the back.
    function useCamera(deviceId) {
        if (deviceId.length === 0 || deviceId === camera.deviceId || root.done) {
            return
        }
        focusHold.stop()
        if (root.facing === Viewfinder.backFace && camera.deviceId.length > 0) {
            root.lastBackCamera = camera.deviceId
        }
        if (root.backCameras.indexOf(deviceId) >= 0) {
            root.lastBackCamera = deviceId
        }
        camera.deviceId = deviceId
        camera.digitalZoom = 1
    }

    /// Zoom to `value`, inside what the camera can do.
    function zoomTo(value) {
        camera.digitalZoom = Viewfinder.clampZoom(value, camera.maximumDigitalZoom)
        zoomIndicator.show()
    }

    Camera {
        id: camera
        objectName: "camera"
        // The video pipeline is where continuous autofocus runs, and
        // where the flash is a torch.
        captureMode: Camera.CaptureVideo
        flash.mode: root.torch && root.facing === Viewfinder.backFace
                    ? Viewfinder.flashTorch : Viewfinder.flashOff
        // Continuous, except for the few seconds after a tap: a search in
        // continuous video focus locks the lens where it is rather than
        // looking, so a tap switches to plain autofocus, which does look,
        // and switches back once the hold is over.
        focus {
            focusMode: focusHold.running ? Camera.FocusAuto : Camera.FocusContinuous
            focusPointMode: focusHold.running ? Camera.FocusPointCustom : Camera.FocusPointAuto
        }
        // The resolution is picked as soon as the camera can say what it
        // has, which is before it goes active.
        onCameraStatusChanged: root.chooseViewfinder()
    }

    /// The long side this view wants from the camera and from a grab.
    ///
    /// A decoder reads a symbol out of the pixels each module lands in,
    /// and wants three or four of them across a module to tell one from
    /// its neighbour. The code a device shows while offering a profile
    /// carries an address and a one-time secret, so it is a dense symbol
    /// -- around seventy modules across with its quiet zone. At 640 a
    /// code filling half the frame put under three pixels in a module
    /// and would not read at all; the room here is what makes that
    /// closer to six.
    readonly property int wantedLongSide: 1280

    /// Set once the camera has been asked for a resolution, so a status
    /// change later does not ask again.
    property bool viewfinderChosen: false

    /// Ask the camera for the smallest viewfinder that is still big
    /// enough, or the biggest it has when none of them are.
    ///
    /// Asked for rather than assumed: the platform's own default is
    /// whatever is cheapest to run, and no grab can put back detail the
    /// camera never captured. Guarded all the way down, because a
    /// camera that cannot answer should leave the view working on
    /// whatever it does give.
    function chooseViewfinder() {
        if (root.viewfinderChosen
                || typeof camera.supportedViewfinderResolutions !== "function") {
            return
        }
        var offered = camera.supportedViewfinderResolutions()
        if (!offered || offered.length === 0) {
            return
        }
        var enough = null
        var largest = null
        for (var i = 0; i < offered.length; i++) {
            var size = offered[i]
            var longest = Math.max(size.width, size.height)
            if (longest <= 0) {
                continue
            }
            if (!largest || longest > Math.max(largest.width, largest.height)) {
                largest = size
            }
            if (longest >= root.wantedLongSide
                    && (!enough
                        || longest < Math.max(enough.width, enough.height))) {
                enough = size
            }
        }
        var pick = enough || largest
        if (!pick) {
            return
        }
        camera.viewfinder.resolution = Qt.size(pick.width, pick.height)
        root.viewfinderChosen = true
    }

    /// Focus on a point of the viewfinder, given as a fraction of its
    /// width and height. The hold starts first, so the camera is in
    /// plain autofocus on that point before the search is asked for.
    /// Unlocked first: a search that finds focus locks it, and a locked
    /// focus is a fixed one.
    function focusAt(x, y) {
        focusHold.restart()
        camera.unlock()
        camera.focus.customFocusPoint = Qt.point(x, y)
        camera.searchAndLock()
    }

    /// A tap on the viewfinder at `x`, `y` in its own coordinates: the
    /// ring goes under the finger, and the camera is told the point in
    /// its own frame.
    function tapAt(x, y) {
        var mapped = typeof viewfinder.mapPointToSourceNormalized === "function"
                ? viewfinder.mapPointToSourceNormalized(Qt.point(x, y)) : null
        var point = Viewfinder.focusPoint(mapped, x, y, viewfinder.width, viewfinder.height)
        if (!point) {
            return
        }
        reticle.at = Qt.point(x, y)
        root.focusAt(point.x, point.y)
    }

    // How long a tap's focus is kept. Once it is over the lock comes off
    // and continuous focus takes the lens back: a lock kept for good is
    // a fixed focus, which the next code held up at another distance
    // would not be read through.
    Timer {
        id: focusHold
        objectName: "focusHold"
        interval: 5000
        onTriggered: camera.unlock()
    }

    // The camera runs only while this view is the one on screen.
    //
    // The resolution is asked for here as well as on a status change: a
    // camera that is already loaded has nothing left to change, and one
    // that is not answers with nothing until it is. Whichever comes
    // first settles it.
    onActiveChanged: {
        if (root.active && !root.done) {
            root.chooseViewfinder()
            camera.start()
        } else {
            camera.stop()
        }
    }

    /// The long side of a grabbed frame: the same as the viewfinder's,
    /// since grabbing smaller throws away the detail just asked for.
    ///
    /// It costs decode time, which the grabber already handles: the next
    /// frame is taken only once the last has been read, so a slower
    /// decode scans a little less often rather than queueing up.
    readonly property int grabLongSide: root.wantedLongSide

    /// That long side, in the viewfinder's own shape. A grab to a fixed
    /// square stretches the modules by whatever the viewfinder is not
    /// square by, and a decoder reading a square symbol as an oblong one
    /// has that much less to work with.
    function grabSize() {
        var longest = Math.max(viewfinder.width, viewfinder.height)
        if (longest <= 0) {
            return Qt.size(root.grabLongSide, root.grabLongSide)
        }
        // Never upscale: a small viewfinder has no more detail to give.
        var ratio = Math.min(1, root.grabLongSide / longest)
        return Qt.size(Math.max(1, Math.round(viewfinder.width * ratio)),
                       Math.max(1, Math.round(viewfinder.height * ratio)))
    }

    Timer {
        id: grabber
        objectName: "grabber"
        interval: 300
        repeat: true
        running: root.active && !scanner.busy && !root.done
        onTriggered: {
            var path = scanner.frame_path()
            if (path.length === 0) {
                return
            }
            viewfinder.grabToImage(function(result) {
                if (result.saveToFile(path)) {
                    root.framesTried += 1
                    scanner.decode(path)
                }
            }, root.grabSize())
        }
    }

    VideoOutput {
        id: viewfinder
        objectName: "viewfinder"
        anchors.fill: parent
        source: camera
        orientation: Viewfinder.pageTurn(root.pageOrientation)
        // Filled rather than fitted. Fitted leaves bars down the sides
        // of a tall view, and those bars are grabbed and decoded along
        // with the picture -- pixels the code does not get. It also
        // means what the reader sees is what is being read.
        fillMode: VideoOutput.PreserveAspectCrop

        // Pinch to zoom, and tap to focus on what is under the finger.
        PinchArea {
            objectName: "pinchArea"
            anchors.fill: parent
            enabled: !root.done
            onPinchUpdated: root.zoomTo(Viewfinder.pinchZoom(camera.digitalZoom,
                                                             camera.maximumDigitalZoom,
                                                             pinch.scale, pinch.previousScale))

            MouseArea {
                objectName: "focusTap"
                anchors.fill: parent
                onClicked: root.tapAt(mouse.x, mouse.y)
            }
        }
    }

    // Beside the viewfinder rather than in it, which the viewfinder
    // shares its coordinates with: a frame is grabbed off the viewfinder
    // and everything in it, and a ring drawn over the code is a ring the
    // decoder has to read through.
    FocusReticle {
        id: reticle
        objectName: "reticle"
        visible: focusHold.running && !root.done
        locked: camera.lockStatus === Viewfinder.lockLocked
    }

    ZoomIndicator {
        id: zoomIndicator
        objectName: "zoomIndicator"
        anchors {
            horizontalCenter: parent.horizontalCenter
            top: hint.bottom
            topMargin: Theme.paddingLarge
        }
        zoom: camera.digitalZoom
        maximumZoom: camera.maximumDigitalZoom
    }

    // The camera's own controls, over the bottom of the picture and above
    // the way to type the link: the torch on the left, the zoom step in
    // the middle with the back cameras' rings above it, the other side on
    // the right. Gone once a code is read, and while the link is typed.
    Item {
        id: cameraControls
        objectName: "cameraControls"
        visible: !root.done && !root.typing
        anchors {
            left: parent.left
            right: parent.right
            bottom: typeLinkButton.visible ? typeLinkButton.top : parent.bottom
            bottomMargin: Theme.paddingLarge
        }
        height: Theme.itemSizeMedium

        IconButton {
            objectName: "torchButton"
            anchors {
                left: parent.left
                leftMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            visible: root.facing === Viewfinder.backFace
            icon.source: Viewfinder.flashIcon(root.torch ? Viewfinder.flashTorch
                                                         : Viewfinder.flashOff)
            onClicked: root.torch = !root.torch
        }

        CameraLensToggle {
            objectName: "lensToggle"
            anchors {
                horizontalCenter: parent.horizontalCenter
                bottom: zoomButton.top
                bottomMargin: Theme.paddingSmall
            }
            cameras: root.backCameras
            labels: Settings.backLensLabels
            current: camera.deviceId
            visible: root.backCameras.length > 1 && root.facing === Viewfinder.backFace
            onSelected: root.useCamera(deviceId)
        }

        // The zoom, one step at a time: one, two, four times, and back.
        MouseArea {
            id: zoomButton
            objectName: "zoomButton"
            anchors.centerIn: parent
            width: Theme.itemSizeSmall
            height: width
            visible: camera.maximumDigitalZoom > 1
            onClicked: root.zoomTo(Viewfinder.nextZoomPreset(camera.digitalZoom,
                                                             camera.maximumDigitalZoom))

            Rectangle {
                anchors.fill: parent
                radius: width / 2
                color: Qt.rgba(0, 0, 0, 0.4)
                border.width: 2
                border.color: zoomButton.pressed ? Theme.highlightColor : "white"
            }

            Label {
                objectName: "zoomLabel"
                anchors.centerIn: parent
                color: zoomButton.pressed ? Theme.highlightColor : "white"
                font.pixelSize: Theme.fontSizeSmall
                text: Viewfinder.zoomText(camera.digitalZoom)
            }
        }

        IconButton {
            objectName: "flipButton"
            anchors {
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            visible: root.otherCamera.length > 0
            icon.source: "image://theme/icon-camera-switch"
            onClicked: root.useCamera(root.otherCamera)
        }
    }

    // The code was read and the host is acting on it.
    BusyIndicator {
        objectName: "acting"
        anchors.centerIn: parent
        running: root.done
        size: BusyIndicatorSize.Large
    }

    /// How many frames have been read and found nothing. Shown as a
    /// spinner rather than a number: what the reader needs to know is
    /// that the app is looking, not how hard.
    property int framesTried: 0

    // Beside the line, inside the page margin rather than anchored off
    // the end of it: anchored to the line's own left edge, it hung half
    // off the side of the screen.
    BusyIndicator {
        id: looking
        objectName: "looking"
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            verticalCenter: hint.verticalCenter
        }
        running: root.active && !root.done && root.framesTried > 0
        size: BusyIndicatorSize.Small
    }

    // The other way in, where a thumb finds it: a button under the
    // viewfinder that opens the panel for the link the code would carry,
    // typed or pasted. Over the viewfinder rather than instead of it.
    Button {
        id: typeLinkButton
        objectName: "typeLinkButton"
        visible: root.offerLink && !root.typing && !root.done
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge
        }
        text: root.linkButtonText
        onClicked: root.typeLink()
    }

    Column {
        id: linkPanel
        objectName: "linkPanel"
        visible: root.offerLink && root.typing && !root.done
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge
        }
        spacing: Theme.paddingSmall

        Rectangle {
            width: parent.width
            height: linkField.height + followButton.height + 2 * Theme.paddingMedium
            color: Theme.rgba(Theme.highlightDimmerColor, 0.8)

            TextField {
                id: linkField
                objectName: "linkField"
                anchors {
                    left: parent.left
                    right: parent.right
                    top: parent.top
                    topMargin: Theme.paddingMedium
                }
                label: root.linkLabel
                placeholderText: root.linkPlaceholder
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
            }

            Button {
                id: followButton
                objectName: "followButton"
                anchors {
                    horizontalCenter: parent.horizontalCenter
                    top: linkField.bottom
                }
                text: root.linkActionText
                enabled: linkField.text.trim().length > 0
                onClicked: root.useLink()
            }
        }
    }

    // At the top rather than the foot: the phone is held up, the code
    // is in the middle of the picture, and the words about it belong
    // where the eye already is rather than down by the hand.
    Label {
        id: hint
        objectName: "hint"
        anchors {
            left: looking.right
            leftMargin: Theme.paddingMedium
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            top: parent.top
            topMargin: root.hintTopMargin + Theme.paddingMedium
        }
        wrapMode: Text.Wrap
        color: Theme.secondaryHighlightColor
        visible: !root.done
        text: root.typing
              ? qsTr("Or point the camera at the code")
              : root.hintText
    }
}
