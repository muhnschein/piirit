// 2.5, not 2.0: Image.autoTransform; see AttachmentPreview.qml.
import QtQuick 2.5
import Sailfish.Silica 1.0
import "../components"

/*
 * One picture, big as screen allows; here, not system handler (leaves Piirit,
 * fails on some devices); pull-down keeps way to other apps. Fit at zoom 1;
 * content is larger of picture and view: no pan until needed, centred.
 *
 * Two steps: tapped row's decode is cache hit, drawn first, nothing decoding
 * in transition (the reported stutter); full decode, GIF movie once in place.
 *
 * Save copies to Piirit's Downloads (AttachmentSaver): chat files in core's
 * dir go with app.
 */
Page {
    id: page

    /// File, already encoded: see AttachmentPreview.fileUrl.
    property url fileUrl
    property string fileName
    /// Image, Gif or Sticker: core enum.
    property string viewType: "Image"

    readonly property bool isAnimated: page.viewType === "Gif"
    /// Width conversation drew it at; 0 if not from there. Row still under page,
    /// so same size is Qt cache hit: first frame free.
    property real previewWidth: 0

    /// Page in place (transition over, or none). Until then decode and upload drop
    /// transition frames: full picture, GIF wait; row's decode carries page.
    property bool settled: false
    onStatusChanged: if (page.status === PageStatus.Active) page.settled = true
    Component.onCompleted: if (page.status === PageStatus.Active) page.settled = true

    /// Shape layout uses: full picture once decoded, else row's.
    readonly property Item measured:
        picture.implicitWidth > 0 && picture.implicitHeight > 0 ? picture : preview

    /// Draw scale vs fitted; 1 fits.
    property real zoom: 1
    readonly property real maximumZoom: 5

    /// Fitted width. Page width until decoded (no divide by zero).
    readonly property real fittedWidth: {
        if (page.measured.implicitWidth <= 0 || page.measured.implicitHeight <= 0) {
            return page.width
        }
        return Math.min(page.width,
                        page.height * page.measured.implicitWidth
                        / page.measured.implicitHeight)
    }
    readonly property real fittedHeight:
        page.measured.implicitWidth > 0
        ? page.fittedWidth * page.measured.implicitHeight / page.measured.implicitWidth
        : page.height

    /// Decode box: twice page, so zoom 2 stays sharp. Bigger wastes memory and
    /// upload; past GPU texture limit Qt rescales on render thread (stall). Per
    /// side, Qt keeps shape: one bound of twice long edge on both let landscape
    /// photo through whole (12 MP, 48 MB for 1080 wide). Qt 5.6 fits stored pixels
    /// before orientation turn: upright 4:3 stored sideways gets 1.5x, not 2x.
    /// Screen size until page has one.
    readonly property int decodeWidth: 2 * (page.width > 0 ? page.width : Screen.width)
    readonly property int decodeHeight: 2 * (page.height > 0 ? page.height : Screen.height)

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
        var next = Math.max(1, Math.min(page.maximumZoom, target))
        var wide = page.fittedWidth * page.zoom
        var high = page.fittedHeight * page.zoom
        if (wide <= 0 || high <= 0 || next === page.zoom) {
            return
        }
        // Point's place in picture, 0..1.
        var across = (flick.contentX + viewX - page.inset(wide, flick.width)) / wide
        var down = (flick.contentY + viewY - page.inset(high, flick.height)) / high

        page.zoom = next

        var wideAfter = page.fittedWidth * next
        var highAfter = page.fittedHeight * next
        // Where it must sit on screen.
        flick.contentX = page.within(
            page.inset(wideAfter, flick.width) + across * wideAfter - viewX,
            Math.max(wideAfter, flick.width) - flick.width)
        flick.contentY = page.within(
            page.inset(highAfter, flick.height) + down * highAfter - viewY,
            Math.max(highAfter, flick.height) - flick.height)
    }

    /// Double tap: all the way in on tapped point, or all the way out.
    function toggleZoom(viewX, viewY) {
        page.zoomAt(page.zoom > 1 ? 1 : 3, viewX, viewY)
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

        PullDownMenu {
            MenuItem {
                objectName: "openExternally"
                //: Hands the attachment to whatever else on the phone
                //: handles files of its kind.
                text: qsTr("Open in another app")
                onClicked: Qt.openUrlExternally(page.fileUrl)
            }
            MenuItem {
                objectName: "saveToDevice"
                text: qsTr("Save to device")
                // No second copy while one is made.
                enabled: !saver.busy
                onClicked: saver.keep(page.fileUrl, page.fileName)
            }
        }

        PinchArea {
            id: pincher
            width: flick.contentWidth
            height: flick.contentHeight
            pinch.minimumScale: 1
            pinch.maximumScale: page.maximumZoom

            /// Zoom at finger-down: pinch scale is relative to its start.
            property real startZoom: 1
            /// Screen point at finger-down: picture stays still under it.
            property real focusX: 0
            property real focusY: 0

            onPinchStarted: {
                pincher.startZoom = page.zoom
                // Content coordinates; zoomAt takes view coordinates.
                pincher.focusX = pinch.startCenter.x - flick.contentX
                pincher.focusY = pinch.startCenter.y - flick.contentY
            }
            onPinchUpdated: page.zoomAt(pincher.startZoom * pinch.scale,
                                        pincher.focusX, pincher.focusY)

            Item {
                id: frame
                width: page.fittedWidth * page.zoom
                height: page.fittedHeight * page.zoom
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
                    sourceSize.width: page.previewWidth
                    source: page.previewWidth > 0 ? page.fileUrl : ""
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
                    sourceSize.width: page.decodeWidth
                    sourceSize.height: page.decodeHeight
                    // Once settled if preview carries page; else at once (empty page worse than
                    // dropped frame).
                    source: page.settled || page.previewWidth <= 0 ? page.fileUrl : ""
                }

                AnimatedImage {
                    objectName: "fullAnimation"
                    anchors.fill: parent
                    visible: page.isAnimated
                    fillMode: Image.PreserveAspectFit
                    // Frames decoded as they come, not whole movie kept (AttachmentPreview).
                    cache: false
                    playing: visible
                    // Movie decodes on main thread: starts after transition.
                    source: visible && page.settled ? page.fileUrl : ""
                }

                MouseArea {
                    id: tap
                    anchors.fill: parent
                    onDoubleClicked: {
                        var point = tap.mapToItem(flick, mouse.x, mouse.y)
                        page.toggleZoom(point.x, point.y)
                    }
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
