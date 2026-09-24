// 2.5 rather than 2.0 for Image.autoTransform; see AttachmentPreview.qml.
import QtQuick 2.5
import Sailfish.Silica 1.0
import "../components"

/*
 * One picture, as big as the screen will show it.
 *
 * Shown here rather than handed to whatever the system thinks handles the
 * type, which leaves Piirit and fails on a device. A page and a
 * Flickable; the way out to another app stays in the pull-down for the
 * cases this cannot do anything with.
 *
 * The picture is fitted at zoom 1 and multiplied from there, and the
 * flickable's content is the larger of the picture and the view -- so
 * panning does nothing until there is something to pan, and the picture
 * stays centred while there is not.
 *
 * It arrives in two steps. The row that was tapped has already decoded
 * the picture at its own width, and that decode is what this page draws
 * first: a cache hit, so the page comes in with the picture on it and
 * nothing decoding behind the transition. The full-size decode, and a
 * GIF's movie, start once the page is in place. Decoding during the
 * transition was the stutter reported on opening a picture.
 *
 * Saving copies the file into Piirit's folder in Downloads, where every
 * copy goes (AttachmentSaver): what the chat holds lives in the core's
 * own directory and goes with the app.
 */
Page {
    id: page

    /// The file, already encoded: see AttachmentPreview.fileUrl for why the
    /// path is not concatenated into one here.
    property url fileUrl
    property string fileName
    /// Image, Gif or Sticker -- the core's own enum, as everywhere else.
    property string viewType: "Image"

    readonly property bool isAnimated: page.viewType === "Gif"
    /// How wide the conversation drew this picture, 0 when it did not
    /// come from there. The row's decode at that width is still in Qt's
    /// cache -- the row is on the page underneath -- and asking for the
    /// same size is what makes the first frame here cost nothing.
    property real previewWidth: 0

    /// Whether the page is in place: its transition is over, or there was
    /// none. Until then the reader is watching the transition, and a
    /// decode and a texture upload in the middle of it are the frames it
    /// drops. The full picture, and a GIF's movie, wait for this; the
    /// row's decode carries the page across.
    property bool settled: false
    onStatusChanged: if (page.status === PageStatus.Active) page.settled = true
    Component.onCompleted: if (page.status === PageStatus.Active) page.settled = true

    /// The picture whose shape the page is laid out from: the full one
    /// once it is decoded, the row's until then. Same shape either way.
    readonly property Item measured:
        picture.implicitWidth > 0 && picture.implicitHeight > 0 ? picture : preview

    /// How much bigger than fitted the picture is drawn. 1 fits it.
    property real zoom: 1
    readonly property real maximumZoom: 5

    /// The width the picture is drawn at when it is fitted to the page.
    ///
    /// Falls back to the page's own width until the picture has been
    /// decoded, which is also what keeps this from dividing by zero.
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

    /// The most pixels a picture is decoded to on its long edge: twice the
    /// screen's, so a zoom of two is still sharp. Beyond that a bigger
    /// decode is memory and upload time for nothing the eye can see --
    /// and past the GPU's texture limit Qt scales it down again on the
    /// render thread, which is a stall of its own.
    readonly property int decodeBound: 2 * Math.max(Screen.width, Screen.height)

    /// Kept inside the content. The flickable would do this itself, but
    /// only once it has been laid out, and this runs before that.
    function within(value, limit) {
        return Math.max(0, Math.min(Math.max(0, limit), value))
    }

    /// Where the picture's left edge sits when it is narrower than the view.
    function inset(size, viewport) {
        return Math.max(0, (Math.max(size, viewport) - size) / 2)
    }

    /// Change the zoom, keeping whatever is under (`viewX`, `viewY`) under
    /// it afterwards.
    ///
    /// The coordinates are in the view, not in the picture. Without this
    /// the picture simply grows about its own top-left corner, and zooming
    /// in on a face in the bottom right takes the reader to the top left
    /// and leaves them to find their way back.
    function zoomAt(target, viewX, viewY) {
        var next = Math.max(1, Math.min(page.maximumZoom, target))
        var wide = page.fittedWidth * page.zoom
        var high = page.fittedHeight * page.zoom
        if (wide <= 0 || high <= 0 || next === page.zoom) {
            return
        }
        // Where that point sits in the picture, 0..1, before the change.
        var across = (flick.contentX + viewX - page.inset(wide, flick.width)) / wide
        var down = (flick.contentY + viewY - page.inset(high, flick.height)) / high

        page.zoom = next

        var wideAfter = page.fittedWidth * next
        var highAfter = page.fittedHeight * next
        // And where it has to be for that point to be under the same place
        // on the screen.
        flick.contentX = page.within(
            page.inset(wideAfter, flick.width) + across * wideAfter - viewX,
            Math.max(wideAfter, flick.width) - flick.width)
        flick.contentY = page.within(
            page.inset(highAfter, flick.height) + down * highAfter - viewY,
            Math.max(highAfter, flick.height) - flick.height)
    }

    /// What a double tap does: all the way in on what was tapped, or all
    /// the way back out.
    function toggleZoom(viewX, viewY) {
        page.zoomAt(page.zoom > 1 ? 1 : 3, viewX, viewY)
    }

    // A copy the reader keeps, where every copy goes: see
    // AttachmentSaver.
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

    // Behind the picture rather than the theme's own background: a photo
    // with anything light in it reads better against black, and the bars
    // beside a picture that does not fill the screen are not the page.
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
                onClicked: saver.keep(page.fileUrl, page.fileName)
            }
        }

        PinchArea {
            id: pincher
            width: flick.contentWidth
            height: flick.contentHeight
            pinch.minimumScale: 1
            pinch.maximumScale: page.maximumZoom

            /// Where the zoom was when the fingers went down: a pinch
            /// reports its scale relative to its own start, not to the
            /// picture.
            property real startZoom: 1
            /// And where on the screen they went down, which is the point
            /// the picture has to stay still under.
            property real focusX: 0
            property real focusY: 0

            onPinchStarted: {
                pincher.startZoom = page.zoom
                // This area is the flickable's content, so its coordinates
                // are content coordinates; the view's are what zoomAt
                // wants.
                pincher.focusX = pinch.startCenter.x - flick.contentX
                pincher.focusY = pinch.startCenter.y - flick.contentY
            }
            onPinchUpdated: page.zoomAt(pincher.startZoom * pinch.scale,
                                        pincher.focusX, pincher.focusY)

            Item {
                id: frame
                width: page.fittedWidth * page.zoom
                height: page.fittedHeight * page.zoom
                // Centred while the picture is smaller than the view, and
                // at the origin once it is bigger and the flickable has
                // taken over.
                x: Math.max(0, (flick.contentWidth - frame.width) / 2)
                y: Math.max(0, (flick.contentHeight - frame.height) / 2)

                // The row's decode of the same picture, at the width the
                // row drew it: a cache hit, so it is on screen from the
                // first frame of the transition with nothing decoded.
                // Underneath the full picture, and gone once that is there.
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

                // The same layering as the message row: an Image reads the
                // first frame of a GIF whatever happens to its movie, so it
                // sizes the page and shows the still, and the animation
                // covers it when there is one to play. See
                // AttachmentPreview.
                Image {
                    id: picture
                    objectName: "fullPicture"
                    anchors.fill: parent
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    autoTransform: true
                    sourceSize.width: page.decodeBound
                    sourceSize.height: page.decodeBound
                    // Once the page is in place, when there is a preview
                    // to carry it there; at once when there is not, since
                    // an empty page is worse than a dropped frame.
                    source: page.settled || page.previewWidth <= 0 ? page.fileUrl : ""
                }

                AnimatedImage {
                    objectName: "fullAnimation"
                    anchors.fill: parent
                    visible: page.isAnimated
                    fillMode: Image.PreserveAspectFit
                    playing: visible
                    // The movie decodes its frames on the main thread as
                    // they come round, so it starts once the transition is
                    // out of the way; the still shows its first frame
                    // until then.
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

        // Only while the picture is still coming and nothing stands in
        // for it: a decode that fails leaves the page black, and a spinner
        // that never stops would say it was still trying.
        BusyIndicator {
            objectName: "pictureBusy"
            anchors.centerIn: parent
            size: BusyIndicatorSize.Large
            running: picture.status === Image.Loading
                     && preview.status !== Image.Ready
        }
    }

    // Where the copy went, or why there is none. Over the picture rather
    // than in the flickable, so panning does not move it.
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
