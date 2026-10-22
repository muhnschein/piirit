// 2.5, not 2.0: Image.autoTransform; see AttachmentPreview.qml.
import QtQuick 2.5
import Sailfish.Silica 1.0
import "."

/*
 * One picture, big as view allows: PicturePage alone, GalleryPage one per slide. Fit at zoom
 * 1; content is larger of picture and view: no pan until needed, centred. Pull-down: other
 * apps, Save to device.
 *
 * Two steps: tapped row's decode is cache hit, drawn first, nothing decoding in transition
 * (the reported stutter); full decode, GIF movie once `settled`.
 *
 * Save copies to Piirit's Downloads (AttachmentSaver): chat files in core's dir go with app.
 */
Item {
    id: root

    /// File, already encoded: see AttachmentPreview.fileUrl.
    property url fileUrl
    property string fileName
    /// Image, Gif or Sticker: core enum.
    property string viewType: "Image"

    readonly property bool isAnimated: root.viewType === "Gif"
    /// Width conversation drew it at; 0 if not from there. Row still under page,
    /// so same size is Qt cache hit: first frame free.
    property real previewWidth: 0

    /// Page in place (transition over, or none); page says. Until then decode and upload drop
    /// transition frames: full picture, GIF wait; row's decode carries page.
    property bool settled: true

    /// Shape layout uses: full picture once decoded, else row's.
    readonly property Item measured:
        picture.implicitWidth > 0 && picture.implicitHeight > 0 ? picture : preview

    /// Slide on screen. Off: GIF holds first frame (movie decodes on main thread; neighbour's
    /// would stall swipe).
    property bool active: true

    /// Draw scale vs fitted; 1 fits.
    property real zoom: 1
    readonly property real maximumZoom: 5

    /// Zoomed in: pans, so swiping viewer leaves drags to it.
    readonly property bool zoomed: root.zoom > 1

    /// Pull-down there (PicturePage). GalleryPage's icon bar does its work instead: page swipes
    /// down to close.
    property bool menuEnabled: true

    /// Single tap (no second follows): GalleryPage shows or hides its controls.
    signal tapped()

    /// Fitted width. View width until decoded (no divide by zero).
    readonly property real fittedWidth: {
        if (root.measured.implicitWidth <= 0 || root.measured.implicitHeight <= 0) {
            return root.width
        }
        return Math.min(root.width,
                        root.height * root.measured.implicitWidth
                        / root.measured.implicitHeight)
    }
    readonly property real fittedHeight:
        root.measured.implicitWidth > 0
        ? root.fittedWidth * root.measured.implicitHeight / root.measured.implicitWidth
        : root.height

    /// Decode box: about view's size; twice only once zoomed (`sharp*`). Twice on every
    /// slide was a 14-25 MB texture each, uploaded as it swiped in (lag). Width a
    /// third over view's: Qt 5.6 fits stored pixels before orientation turn, so 4:3
    /// photo stored sideways (phone camera) comes out exactly fitted, not 0.75x
    /// (blurred); upright one 1.33x. Per side, Qt keeps shape: one bound of long edge
    /// on both let landscape photo through whole (12 MP, 48 MB for 1080 wide).
    /// Screen size until view has one.
    readonly property int decodeWidth:
        Math.round(4 / 3 * (root.width > 0 ? root.width : Screen.width))
    readonly property int decodeHeight: root.height > 0 ? root.height : Screen.height
    /// Zoomed copy's box: twice view, so zoom 2 stays sharp. Bigger wastes memory
    /// and upload; past GPU texture limit Qt rescales on render thread (stall).
    readonly property int sharpWidth: 2 * (root.width > 0 ? root.width : Screen.width)
    readonly property int sharpHeight: 2 * root.decodeHeight

    /// Kept inside content: flickable does this after layout, this before.
    function within(value, limit) {
        return Math.max(0, Math.min(Math.max(0, limit), value))
    }

    /// Left edge when narrower than view.
    function inset(size, viewport) {
        return Math.max(0, (Math.max(size, viewport) - size) / 2)
    }

    /// Zoom keeping (`viewX`, `viewY`) under it; view coordinates, else zoom grows
    /// from top-left.
    function zoomAt(target, viewX, viewY) {
        var next = Math.max(1, Math.min(root.maximumZoom, target))
        var wide = root.fittedWidth * root.zoom
        var high = root.fittedHeight * root.zoom
        if (wide <= 0 || high <= 0 || next === root.zoom) {
            return
        }
        // Point's place in picture, 0..1.
        var across = (flick.contentX + viewX - root.inset(wide, flick.width)) / wide
        var down = (flick.contentY + viewY - root.inset(high, flick.height)) / high

        root.zoom = next

        var wideAfter = root.fittedWidth * next
        var highAfter = root.fittedHeight * next
        // Where it must sit on screen.
        flick.contentX = root.within(
            root.inset(wideAfter, flick.width) + across * wideAfter - viewX,
            Math.max(wideAfter, flick.width) - flick.width)
        flick.contentY = root.within(
            root.inset(highAfter, flick.height) + down * highAfter - viewY,
            Math.max(highAfter, flick.height) - flick.height)
    }

    /// Double tap: all the way in on tapped point, or all the way out.
    function toggleZoom(viewX, viewY) {
        root.zoomAt(root.zoom > 1 ? 1 : 3, viewX, viewY)
    }

    /// Fitted again (GalleryPage: tap on zoomed picture, as Jolla's).
    function zoomOut() {
        root.zoomAt(1, flick.width / 2, flick.height / 2)
    }

    /// Second tap of a double: zoom, first tap no single one.
    function zoomTap(viewX, viewY) {
        single.stop()
        root.toggleZoom(viewX, viewY)
    }

    // First tap waits out double-tap time: zooming must not flash header.
    Timer {
        id: single
        interval: Qt.styleHints.mouseDoubleClickInterval
        onTriggered: root.tapped()
    }

    // Reader's copy: see AttachmentSaver.
    AttachmentSaver {
        id: saver
        objectName: "saver"
        onSaved: {
            notice.tone = "info"
            notice.show(saver.savedText)
        }
        onError: {
            notice.tone = "error"
            notice.show(message)
        }
    }

    // Behind picture: photos read better on black.
    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    SilicaFlickable {
        id: flick
        objectName: "pictureFlick"
        anchors.fill: parent
        contentWidth: Math.max(flick.width, frame.width)
        contentHeight: Math.max(flick.height, frame.height)
        clip: true
        // No menu, fitted: nothing to pull or pan, so drags are page's (down closes) and
        // list's.
        interactive: root.menuEnabled || root.zoomed

        PullDownMenu {
            id: menu
            objectName: "pictureMenu"
            visible: root.menuEnabled
            MenuItem {
                objectName: "openExternally"
                //: Hands the attachment to whatever else on the phone
                //: handles files of its kind.
                text: qsTr("Open in another app")
                onClicked: Qt.openUrlExternally(root.fileUrl)
            }
            MenuItem {
                objectName: "saveToDevice"
                text: qsTr("Save to device")
                // No second copy while one is made.
                enabled: !saver.busy
                onClicked: saver.keep(root.fileUrl, root.fileName)
            }
        }

        PinchArea {
            id: pincher
            width: flick.contentWidth
            height: flick.contentHeight
            pinch.minimumScale: 1
            pinch.maximumScale: root.maximumZoom

            /// Zoom at finger-down: pinch scale is relative to its start.
            property real startZoom: 1
            /// Screen point at finger-down: picture stays still under it.
            property real focusX: 0
            property real focusY: 0

            onPinchStarted: {
                pincher.startZoom = root.zoom
                // Content coordinates; zoomAt takes view coordinates.
                pincher.focusX = pinch.startCenter.x - flick.contentX
                pincher.focusY = pinch.startCenter.y - flick.contentY
            }
            onPinchUpdated: root.zoomAt(pincher.startZoom * pinch.scale,
                                        pincher.focusX, pincher.focusY)

            // Whole slide, not picture only: black beside it taps too.
            MouseArea {
                id: tap
                objectName: "pictureTap"
                anchors.fill: parent
                onClicked: single.restart()
                onDoubleClicked: {
                    var point = tap.mapToItem(flick, mouse.x, mouse.y)
                    root.zoomTap(point.x, point.y)
                }
            }

            Item {
                id: frame
                width: root.fittedWidth * root.zoom
                height: root.fittedHeight * root.zoom
                // Centred while smaller than view; origin once bigger.
                x: Math.max(0, (flick.contentWidth - frame.width) / 2)
                y: Math.max(0, (flick.contentHeight - frame.height) / 2)

                // Row's decode at its width: cache hit, shown from first frame; gone once full
                // picture in.
                Image {
                    id: preview
                    objectName: "previewPicture"
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    autoTransform: true
                    sourceSize.width: root.previewWidth
                    source: root.previewWidth > 0 ? root.fileUrl : ""
                    visible: picture.status !== Image.Ready
                }

                // Layered as message row: Image shows GIF's first frame, sizes page; animation
                // covers it. See AttachmentPreview.
                Image {
                    id: picture
                    objectName: "fullPicture"
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    autoTransform: true
                    sourceSize.width: root.decodeWidth
                    sourceSize.height: root.decodeHeight
                    // Once settled if preview carries page; else at once (empty page worse than
                    // dropped frame).
                    source: root.settled || root.previewWidth <= 0 ? root.fileUrl : ""
                }

                // Zoomed: twice-size copy over fitted one, once decoded; dropped on zoom out.
                // GIF: animation covers it anyway.
                Image {
                    id: sharp
                    objectName: "sharpPicture"
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    autoTransform: true
                    sourceSize.width: root.sharpWidth
                    sourceSize.height: root.sharpHeight
                    visible: sharp.status === Image.Ready
                    source: root.zoomed && root.settled && !root.isAnimated ? root.fileUrl : ""
                }

                AnimatedImage {
                    objectName: "fullAnimation"
                    anchors.fill: parent
                    visible: root.isAnimated
                    fillMode: Image.PreserveAspectFit
                    // Frames decoded as they come, not whole movie kept (AttachmentPreview).
                    cache: false
                    playing: visible && root.active
                    // Movie decodes on main thread: starts after transition, on screen only.
                    source: visible && root.settled && root.active ? root.fileUrl : ""
                }
            }
        }

        // Only while picture coming: failed decode would spin forever.
        BusyIndicator {
            objectName: "pictureBusy"
            anchors.centerIn: parent
            size: BusyIndicatorSize.Large
            running: picture.status === Image.Loading
                     && preview.status !== Image.Ready
        }
    }

    // Where copy went, or why not. Over picture: pan doesn't move it.
    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 4
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        onDismissed: notice.text = ""
    }
}
