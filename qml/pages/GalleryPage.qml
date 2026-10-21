import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media
import Piirit 1.0

/*
 * Chat's pictures and videos, one per screen, swiped: from conversation row or gallery tile.
 * As Jolla Gallery's full-screen view: swipe up or down closes (Silica's vertical navigation);
 * a tap fades controls in and out (ViewerOverlay: sender and day, close, icon bar). All of chat's
 * gallery (ChatMedia "gallery": pictures, GIFs, videos; no stickers), not only rows loaded; not
 * yet downloaded ones too, as their row says, with Download. Turns with phone.
 *
 * Order as where opened (reference clients alike): conversation oldest left, newest right;
 * gallery grid newest first, so newest left. Model ordered so (`oldest_first`).
 *
 * Tapped one shown alone (`seed`) until list has its id (one core call).
 * Seed's picture is row's cached decode (PictureView); list's slide asks same, so swap draws
 * nothing new. Not in list (sticker, not yet sent): seed stays, nothing to swipe.
 *
 * Zoomed picture pans, list holds still, controls away; tap zooms out. Video plays on its
 * button; playing with controls away, list holds still too (Jolla's).
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

    // Black page: Silica's light-on-dark colours for icons, whatever the ambience.
    palette.colorScheme: Theme.LightOnDark

    // Landscape picture or video wants phone turned (Silica default: upright only).
    allowedOrientations: Orientation.All

    // Sideways swipes are the list's: back is up or down, as Jolla's. Its close button stands
    // for the arrow.
    navigationStyle: PageNavigation.Vertical
    showNavigationIndicator: false

    /// Controls up (ViewerOverlay, video's play button). Tap toggles.
    property bool controlsShown: true

    /// List's slide on screen; null while seed shows alone (no row yet).
    readonly property Item current: page.placed ? slides.currentItem : null
    /// Its video, if one: overlay's seek bar.
    readonly property Item currentVideo: page.placed
                                         ? (page.current ? page.current.video : null)
                                         : (seed.item && page.viewType === "Video" ? seed.item
                                                                                  : null)
    readonly property bool videoPlaying: page.currentVideo !== null && page.currentVideo.playing

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
        // List always left to right, older left: model in shown order. Right-to-left list
        // broke Silica's back transition from chat (slid to a neighbour, then cut).
        oldest_first: !page.newestFirst
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

    // Zoomed in: controls away, picture whole; out again: back (Jolla's).
    readonly property bool currentZoomed: page.current !== null && page.current.zoomed
    onCurrentZoomedChanged: page.controlsShown = !page.currentZoomed

    // Play pressed: full screen for it, as Jolla's.
    onVideoPlayingChanged: if (page.videoPlaying) page.controlsShown = false

    // Controls back while playing: away again after a while, unless seek bar held.
    Timer {
        id: hideControls
        objectName: "hideControls"
        interval: 4000
        running: page.controlsShown && page.videoPlaying && !overlay.seeking
        onTriggered: page.controlsShown = false
    }

    /// Tap on picture or video: zoomed one zooms out, else controls in or out.
    function tapped(view) {
        if (view.zoomed === true) {
            view.zoomOut()
        } else {
            page.controlsShown = !page.controlsShown
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

    /// Overlay's top line: sender, day (phone's short date format, gallery grid captions');
    /// day alone if sender unknown. Empty until row read.
    function caption(slide) {
        if (!slide || slide.timestamp <= 0) {
            return ""
        }
        var day = Qt.formatDate(new Date(slide.timestamp * 1000), Qt.DefaultLocaleShortDate)
        //: Top line of the full-screen viewer: who sent the picture or video, then the day,
        //: such as "Ada Lovelace, 15/11/2023".
        return slide.sender.length > 0 ? qsTr("%1, %2").arg(slide.sender).arg(day) : day
    }

    /// File on screen: slide's, or tapped one's before list has it.
    function shownFile() {
        return page.placed ? (page.current ? page.current.fileUrl : "") : page.fileUrl
    }
    function shownName() {
        return page.placed ? (page.current ? page.current.fileName : "") : page.fileName
    }
    function shownMime() {
        var mime = page.placed && page.current ? page.current.mimeType : ""
        if (mime.length > 0) {
            return mime
        }
        var type = page.placed && page.current ? page.current.viewType : page.viewType
        return type === "Video" ? "video/mp4" : type === "Gif" ? "image/gif" : "image/jpeg"
    }

    /// Sheet over page: video waits (Jolla's).
    function share() {
        var file = String(page.shownFile())
        if (file.length > 0 && shareAction.item) {
            if (page.videoPlaying) {
                page.currentVideo.pause()
            }
            shareAction.item.shareFile(file, page.shownMime())
        }
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
        // Neighbours lie beside screen: unclipped, left one drew over chat as page slid out on
        // Back (slide, then cut).
        clip: true
        model: media.rows
        orientation: ListView.Horizontal
        snapMode: ListView.SnapOneItem
        highlightRangeMode: ListView.StrictlyEnforceRange
        // Swipe is the motion; placing is a jump.
        highlightMoveDuration: 0
        boundsBehavior: Flickable.StopAtBounds
        // One neighbour each side built: decoded before swiped to.
        cacheBuffer: width
        // Zoomed picture takes drags. Video playing full screen holds still (Jolla's).
        interactive: !page.currentZoomed && !(page.videoPlaying && !page.controlsShown)

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
            /// On screen, swipe over, page in place: GIF animates; video off it pauses. Not
            /// mid-swipe: current changes halfway, and work started there stalled the swipe.
            readonly property bool active: slide.ListView.isCurrentItem && !slides.moving
                                           && page.placed && page.settled
            /// Video keeps its file: on screen, or swipe still running (teardown waits).
            readonly property bool kept: slide.ListView.isCurrentItem || slides.moving
            readonly property bool zoomed: shown.item !== null && shown.item.zoomed === true
            /// Video slide's view: overlay's seek bar drives it.
            readonly property Item video: shown.item !== null && slide.viewType === "Video"
                                          && !slide.held ? shown.item : null
            /// Not yet downloaded: row's text and Download stand in (HeldBackView).
            readonly property bool held: model.loaded && model.download_state.length > 0
                                         && model.download_state !== "Done"
            // Overlay's top line: unread row knows none.
            readonly property string sender: model.loaded ? model.sender_name : ""
            readonly property real timestamp: model.loaded ? model.timestamp : 0
            readonly property string mimeType: model.loaded ? model.file_mime : ""
            readonly property int durationMs: model.loaded ? model.duration_ms : 0

            /// Content built. Slide two over enters cache as swipe starts; building it then
            /// (decode, texture upload) landed in swipe's last frames (lag as it eased off).
            /// So: once list still, or on screen, or tapped one. Kept once built.
            property bool built: false
            function build(placed) {
                if (slide.built) {
                    return
                }
                var onScreen = placed && slide.x < slides.contentX + slides.width
                               && slide.x + slide.width > slides.contentX
                if (slide.isTapped || !slides.moving || onScreen) {
                    slide.built = true
                }
            }
            // Not yet placed at completion (x 0 until list lays it out): screen check later.
            Component.onCompleted: slide.build(false)
            Connections {
                target: slides
                onMovingChanged: slide.build(true)
                onContentXChanged: slide.build(true)
            }
            // Unread row: tapped one's from page, else nothing yet.
            readonly property url fileUrl:
                model.loaded ? page.fileUrlOf(model.file_path)
                             : slide.isTapped ? page.fileUrl : ""
            readonly property string fileName:
                model.loaded ? model.file_name : slide.isTapped ? page.fileName : ""
            readonly property string viewType:
                model.loaded ? model.view_type : slide.isTapped ? page.viewType : ""

            // Neighbours built over several frames, not in one during the swipe. Tapped one
            // at once: swap from seed draws nothing new.
            Loader {
                id: shown
                anchors.fill: parent
                active: slide.built
                asynchronous: !slide.isTapped
                sourceComponent: slide.held ? heldSlide
                                 : slide.viewType === "Video" ? videoSlide
                                 : slide.viewType.length > 0 ? pictureSlide : null
            }

            // Fetched: row gets file, slide turns picture or video.
            Component {
                id: heldSlide
                HeldBackView {
                    objectName: "heldView"
                    viewType: slide.viewType
                    text: model.text
                    downloadState: model.download_state
                    onDownloadRequested: media.download_full(slide.messageId)
                }
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
                    menuEnabled: false
                    onTapped: page.tapped(shown.item)
                }
            }

            Component {
                id: videoSlide
                VideoView {
                    objectName: "videoView"
                    fileUrl: slide.fileUrl
                    fileName: slide.fileName
                    mimeType: model.file_mime.length > 0 ? model.file_mime : "video/mp4"
                    active: slide.active
                    kept: slide.kept
                    controlsShown: page.controlsShown
                    // Sender's: seek bar's length till player has file.
                    durationMs: slide.durationMs
                    onTapped: page.tapped(shown.item)
                }
            }
        }
    }

    // Tapped one alone, until list has it. Not yet downloaded one has no file: black till then.
    Loader {
        id: seed
        objectName: "seed"
        anchors.fill: parent
        active: !page.placed && String(page.fileUrl).length > 0
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
            menuEnabled: false
            onTapped: page.tapped(seed.item)
        }
    }

    Component {
        id: videoSeed
        VideoView {
            objectName: "seedVideo"
            fileUrl: page.fileUrl
            fileName: page.fileName
            active: page.settled
            controlsShown: page.controlsShown
            onTapped: page.tapped(seed.item)
        }
    }

    // Over slides, fixed while they swipe.
    ViewerOverlay {
        id: overlay
        objectName: "overlay"
        anchors.fill: parent
        shown: page.controlsShown
        video: page.currentVideo
        // Not yet downloaded one has none.
        hasFile: String(page.shownFile()).length > 0
        caption: page.caption(page.current)
        canShare: shareAction.status === Loader.Ready
        saving: saver.busy
        onCloseRequested: pageStack.pop()
        onShareRequested: page.share()
        onSaveRequested: saver.keep(page.shownFile(), page.shownName())
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

    // Where copy went, or why not: top, over sender and day, a few seconds.
    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "viewerNoticeLabel"
        tone: "info"
        timeout: 4
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
        }
        onDismissed: notice.text = ""
    }

    // Share sheet, loaded not declared: see share/ShareFile.qml.
    Loader {
        id: shareAction
        objectName: "shareLoader"
        source: Qt.resolvedUrl("../share/ShareFile.qml")
    }
}
