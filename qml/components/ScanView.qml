import QtQuick 2.0
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import Piirit 1.0
import "."
import "../js/Viewfinder.js" as Viewfinder

/*
 * QR scanner, or type/paste link (button under viewfinder). Text goes out on
 * `scanned`; host decides. Core checks it first.
 *
 * Own component, loaded by URL (QR page; take-over page). Only file naming a
 * Camera, so a device without one loses the scanner, not the host page. Hosts
 * give it the rest of the page: codes need pixels. Host sets `hintText`,
 * `offerLink`.
 *
 * Frames via `grabToImage` (only way on Qt 5.6 to get viewfinder pixels), to the
 * scanner as a file, one at a time: next grab after last decoded, so slow
 * decodes never queue.
 *
 * View stays (camera off, busy) until host takes it down: Silica drops stack
 * ops during a transition, so self-pop would lose host navigation (e.g. opening
 * the invite's chat). Typed link is a panel, not a dialog, for the same reason.
 *
 * Focus: continuous autofocus; tap focuses a point. No periodic search: video
 * capture maps continuous focus to Android HAL "continuous-video", where a focus
 * trigger locks lens at once, mid-sweep ("continuous-picture" finishes first).
 * Periodic search pinned lens and blurred Jolla Phone viewfinder. CodeReader and
 * Foil Auth also search only on tap.
 *
 * Zoom: pinch, or button stepping 1/2/4x. Bottom controls: torch, back-lens
 * rings, other-side switch (codes on a screen held to the front).
 */
Item {
    id: root

    /// On screen, app in front: host says (Silica keeps background pages active).
    /// Camera runs and grabs only then.
    property bool active: false

    /// Host orientation (Silica `Orientation`). Frame is fixed to phone: turn
    /// back by page.
    property int pageOrientation: Viewfinder.portrait

    /// Read or typed text.
    signal scanned(string text)
    /// Scanner failed on a frame.
    signal failed(string message)

    /// Set once read: in-flight frame can't report it twice.
    property bool done: false
    /// Link panel open. Camera keeps running: reader may still find the code.
    property bool typing: false

    /// Line offset from view top, for host's own words above.
    property real hintTopMargin: 0

    /// Words saying what to point at (host's: invite, or other phone's offer).
    property string hintText: qsTr("Point the camera at someone's invite code")

    /// Code's link can be typed or pasted (every camera here: no reading = dead
    /// end).
    property bool offerLink: true

    /// Typed-half label: invite, or string beside device's code.
    property string linkButtonText: qsTr("Enter invite link")
    property string linkLabel: qsTr("Invite link")
    property string linkPlaceholder: "https://i.delta.chat/..."
    property string linkActionText: qsTr("Connect")

    /// Link-form starts, lower case. Decides only if clipboard is worth
    /// pasting.
    property var linkPrefixes: [
        "https://i.delta.chat/",
        "openpgp4fpr:",
        "dcaccount:",
        "dclogin:"
    ]

    /// Read or typed text, once worth acting on.
    function found(text) {
        if (root.done) {
            return
        }
        root.done = true
        camera.stop()
        root.scanned(text)
    }

    /// Open link panel, prefilled from clipboard if it looks like an invite.
    function typeLink() {
        root.typing = true
        var clip = Clipboard.text
        if (linkField.text.length === 0 && clip.length > 0 && root.looksLikeInvite(clip)) {
            linkField.text = clip
        }
    }

    /// Text is a link form? Keeps shopping list out of field.
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

    /// Device ids per side, platform order.
    readonly property var backCameras:
        Viewfinder.camerasFacing(QtMultimedia.availableCameras, Viewfinder.backFace)
    readonly property var frontCameras:
        Viewfinder.camerasFacing(QtMultimedia.availableCameras, Viewfinder.frontFace)
    readonly property int facing: root.frontCameras.indexOf(camera.deviceId) >= 0
                                  ? Viewfinder.frontFace : Viewfinder.backFace
    property string lastBackCamera: ""
    readonly property string otherCamera:
        Viewfinder.otherSide(QtMultimedia.availableCameras, root.facing, root.lastBackCamera)

    /// Torch on. Back only.
    property bool torch: false

    /// Switch camera. Zoom and tap focus belong to camera they were set on.
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

    /// Zoom to `value`, within camera capability.
    function zoomTo(value) {
        camera.digitalZoom = Viewfinder.clampZoom(value, camera.maximumDigitalZoom)
        zoomIndicator.show()
    }

    Camera {
        id: camera
        objectName: "camera"
        // Autofocus and flash (torch) live in video pipeline.
        captureMode: Camera.CaptureVideo
        flash.mode: root.torch && root.facing === Viewfinder.backFace
                    ? Viewfinder.flashTorch : Viewfinder.flashOff
        // Continuous, except few s after tap: search in continuous video focus locks
        // instead of looking, so tap uses plain autofocus (which looks), then back.
        focus {
            focusMode: focusHold.running ? Camera.FocusAuto : Camera.FocusContinuous
            focusPointMode: focusHold.running ? Camera.FocusPointCustom : Camera.FocusPointAuto
        }
        // Resolution picked once camera reports capabilities.
        onCameraStatusChanged: root.chooseViewfinder()
    }

    /// Long side wanted. Decoder needs 3-4 px per module; dense device code
    /// (~70 modules) gets ~6 px here. 640 gave under 3.
    readonly property int wantedLongSide: 1280

    /// Set once resolution asked.
    property bool viewfinderChosen: false

    /// Smallest viewfinder still big enough, else biggest. Asked, not assumed:
    /// default is cheapest; grab can't restore lost detail. Guarded.
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

    /// Focus on point (fraction of width/height): hold, then search. Unlock
    /// first: found focus locks; locked is fixed.
    function focusAt(x, y) {
        focusHold.restart()
        camera.unlock()
        camera.focus.customFocusPoint = Qt.point(x, y)
        camera.searchAndLock()
    }

    /// Tap at `x`, `y` in view coords: ring under finger, camera gets point.
    function tapAt(x, y) {
        var mapped = typeof viewfinder.mapPointToSourceNormalized === "function"
                ? viewfinder.mapPointToSourceNormalized(Qt.point(x, y)) : null
        var point = Viewfinder.focusPoint(mapped, x, y, viewfinder.width, viewfinder.height)
        if (!point) {
            return
        }
        // Ring is beside view, which may turn.
        var shown = viewfinder.mapToItem(root, x, y)
        reticle.at = Qt.point(shown.x, shown.y)
        root.focusAt(point.x, point.y)
    }

    // Tap focus lapses; permanent lock would fix focus, unreadable at other
    // distances.
    Timer {
        id: focusHold
        objectName: "focusHold"
        interval: 5000
        onTriggered: camera.unlock()
    }

    // Camera runs only while view on screen. Resolution asked here too, for
    // loaded camera.
    onActiveChanged: {
        if (root.active && !root.done) {
            root.chooseViewfinder()
            camera.start()
        } else {
            camera.stop()
        }
    }

    /// Grab's long side = viewfinder's, else detail lost. Next grab waits for
    /// last read.
    readonly property int grabLongSide: root.wantedLongSide

    /// Grab's long side in viewfinder's shape: square grab stretches modules.
    function grabSize() {
        var longest = Math.max(viewfinder.width, viewfinder.height)
        if (longest <= 0) {
            return Qt.size(root.grabLongSide, root.grabLongSide)
        }
        // No upscale: small viewfinder has no extra detail.
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
        // Turned against page, not VideoOutput's `orientation` (Qt 5.6 squeezes
        // picture).
        readonly property int turn: Viewfinder.pageTurn(root.pageOrientation)
        readonly property bool across: turn % 180 !== 0
        anchors.centerIn: parent
        width: across ? parent.height : parent.width
        height: across ? parent.width : parent.height
        rotation: -turn
        source: camera
        // Filled, not fitted: bars on tall view get decoded as pixels with no
        // code.
        fillMode: VideoOutput.PreserveAspectCrop

        // Pinch zoom; tap focus.
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

    // Sibling of viewfinder, not child: grab takes viewfinder and children, so a
    // ring inside would be decoded through.
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

    // Camera controls over picture bottom: torch left, zoom middle (lens rings
    // above), other side right. Gone once read, and while typing link.
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

        // Zoom steps: 1, 2, 4x, back.
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

    // Read; host acting on it.
    BusyIndicator {
        objectName: "acting"
        anchors.centerIn: parent
        running: root.done
        size: BusyIndicatorSize.Large
    }

    /// Frames read, nothing found. Spinner, not number: shows app is looking.
    property int framesTried: 0

    // Inside page margin: anchored to line's left edge it hung off screen.
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

    // Other way in, for thumb: button under viewfinder opens link panel.
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

    // At top: phone held up, code mid-picture; words belong where eye is.
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
