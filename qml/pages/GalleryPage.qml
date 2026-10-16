import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media
import Piirit 1.0

/*
 * Chat's pictures and videos, one per screen, swiped: from conversation row or gallery tile.
 * All of chat's gallery (ChatMedia "gallery": pictures, GIFs, videos; no stickers), not only
 * rows loaded.
 *
 * Order as where opened (reference clients alike): conversation oldest left, newest right;
 * gallery grid newest first, so newest left.
 *
 * Tapped one shown alone (`seed`) until list has its id (one core call).
 * Seed's picture is row's cached decode (PictureView); list's slide asks same, so swap draws
 * nothing new. Not in list (sticker, not yet sent): seed stays, nothing to swipe.
 *
 * Zoomed picture pans, list holds still. Video plays only on screen.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Message tapped: first shown.
    property int messageId
    /// Tapped one's file, name, kind (core enum): seed's, before list has row.
    property url fileUrl
    property string fileName
    property string viewType: "Image"
    /// Width conversation drew it at; 0 if not from there. See PictureView.
    property real previewWidth: 0
    /// Grid order (newest left); else conversation's (oldest left).
    property bool newestFirst: false

    /// Page in place (transition over, or none); decodes, video wait. See PictureView.
    property bool settled: false
    onStatusChanged: if (page.status === PageStatus.Active) page.settled = true
    Component.onCompleted: if (page.status === PageStatus.Active) page.settled = true

    /// Message on screen: tapped one, then where swipes go. List reread keeps it.
    property int shownId: page.messageId
    /// List shows `shownId`; seed gone.
    property bool placed: false

    // Grid's model, own copy: reread on arrival, delete.
    ChatMedia {
        id: media
        objectName: "media"
        account_id: page.accountId
        chat_id: page.chatId
        kind: "gallery"
        onLoaded_changed: page.place()
        // Tapped one arrived late (list read before it): place then.
        onRows_changed: {
            if (!page.placed) {
                page.place()
            }
        }
    }

    Connections {
        target: core
        // Arrival, delete: rows inserted, removed; slide on screen stays (chat_media.rs).
        onCore_event: media.handle_event(context_id, kind, payload_json)
        // Model made before core up: nothing to load.
        onStatus_changed: {
            if (core.status === "ready") {
                media.reload()
            }
        }
    }

    /// List to `shownId` once ids in; again after restart (rows reset).
    function place() {
        if (!media.loaded) {
            return
        }
        var at = media.index_of(page.shownId)
        if (at < 0) {
            return
        }
        slides.currentIndex = at
        slides.positionViewAtIndex(at, ListView.SnapPosition)
        page.placed = true
    }

    /// Path as URL views load; empty until row loaded.
    function fileUrlOf(path) {
        return path.length > 0 ? Qt.resolvedUrl(Media.fileUrl(path)) : ""
    }

    // Behind slides: photos read better on black.
    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    ListView {
        id: slides
        objectName: "slides"
        anchors.fill: parent
        visible: page.placed
        model: media.rows
        orientation: ListView.Horizontal
        // Index 0 = newest: right-to-left puts it rightmost.
        layoutDirection: page.newestFirst ? Qt.LeftToRight : Qt.RightToLeft
        snapMode: ListView.SnapOneItem
        highlightRangeMode: ListView.StrictlyEnforceRange
        // Swipe is the motion; placing is a jump.
        highlightMoveDuration: 0
        boundsBehavior: Flickable.StopAtBounds
        // One neighbour each side built: decoded before swiped to.
        cacheBuffer: width
        // Zoomed picture takes drags.
        interactive: !(slides.currentItem && slides.currentItem.zoomed)

        // Swipe, or neighbour after delete. Not while ids land (row 0 then, not reader's).
        onCurrentIndexChanged: {
            if (page.placed && media.loaded && slides.currentItem) {
                page.shownId = slides.currentItem.messageId
            }
        }

        delegate: Item {
            id: slide
            objectName: "slide" + model.message_id
            width: slides.width
            height: slides.height

            readonly property int messageId: model.message_id
            readonly property bool isTapped: model.message_id === page.messageId
            /// On screen, page in place: plays, animates.
            readonly property bool active: slide.ListView.isCurrentItem && page.placed
                                           && page.settled
            readonly property bool zoomed: shown.item !== null && shown.item.zoomed === true
            // Unread row: tapped one's from page, else nothing yet.
            readonly property url fileUrl:
                model.loaded ? page.fileUrlOf(model.file_path)
                             : slide.isTapped ? page.fileUrl : ""
            readonly property string fileName:
                model.loaded ? model.file_name : slide.isTapped ? page.fileName : ""
            readonly property string viewType:
                model.loaded ? model.view_type : slide.isTapped ? page.viewType : ""

            Loader {
                id: shown
                anchors.fill: parent
                sourceComponent: slide.viewType === "Video" ? videoSlide
                                 : slide.viewType.length > 0 ? pictureSlide : null
            }

            Component {
                id: pictureSlide
                PictureView {
                    objectName: "pictureView"
                    fileUrl: slide.fileUrl
                    fileName: slide.fileName
                    viewType: slide.viewType
                    // Row's cached decode: tapped one only.
                    previewWidth: slide.isTapped ? page.previewWidth : 0
                    settled: page.settled
                    active: slide.active
                }
            }

            Component {
                id: videoSlide
                VideoView {
                    objectName: "videoView"
                    fileUrl: slide.fileUrl
                    fileName: slide.fileName
                    active: slide.active
                }
            }
        }
    }

    // Tapped one alone, until list has it.
    Loader {
        id: seed
        objectName: "seed"
        anchors.fill: parent
        active: !page.placed
        sourceComponent: page.viewType === "Video" ? videoSeed : pictureSeed
    }

    Component {
        id: pictureSeed
        PictureView {
            objectName: "seedPicture"
            fileUrl: page.fileUrl
            fileName: page.fileName
            viewType: page.viewType
            previewWidth: page.previewWidth
            settled: page.settled
        }
    }

    Component {
        id: videoSeed
        VideoView {
            objectName: "seedVideo"
            fileUrl: page.fileUrl
            fileName: page.fileName
            active: page.settled
        }
    }
}
