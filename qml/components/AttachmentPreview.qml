// 2.5: Image.autoTransform needs it for photo orientation. Harbour max 2.6.
import QtQuick 2.5
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0
import "../js/Emoji.js" as Emoji
import "../js/Format.js" as Format

/*
 * Renderer per core `viewType`; never inspects files; fallback names file. Own component
 * (MessageDelegate too long), testable per kind. Bindings, not Column: positioner sizes in
 * polish pass, never headless. Renderers at y 0; height is loaded one's. Each kind behind own
 * Loader: scrolled row builds only what it shows. Nothing starts on its own.
 */
Item {
    id: root

    /// Path in blob dir.
    property string filePath: ""
    /// Display name.
    property string fileName: ""
    /// Core MIME type; empty if none.
    property string fileMime: ""
    /// Bytes; 0 unknown. `real`: QML has no 64-bit int; large file overflows.
    property real fileBytes: 0
    /// Core kind: Text, Image, Gif, Sticker, Audio, Voice, Video, File, Call, Webxdc, Vcard.
    property string viewType: "Text"
    /// Pixel size; often 0 (core 0x0 for valid GIF): check before dividing.
    property int imageWidth: 0
    property int imageHeight: 0
    /// Unseen: arrived while open, or unread at open. Only such GIFs autoplay.
    property bool isNew: false
    /// Sound ms if core knows (sender writes it for recordings, none for handed file), often 0.
    /// Shown till first play builds player.
    property int durationMs: 0
    /// List moving: GIF holds frame instead of decoding on scroll thread.
    property bool scrolling: false
    /// Shared contact, for `Vcard`.
    property string vcardName: ""
    property string vcardAddr: ""
    property string vcardColor: ""
    /// webxdc app for `Webxdc`; no name: unreadable .xdc, drawn as file.
    property string webxdcName: ""
    property string webxdcDocument: ""
    property string webxdcSummary: ""
    property string webxdcIcon: ""
    /// webxdc apps on; off: `.xdc` drawn as file, opened by system. Page binds it.
    property bool appsEnabled: false
    /// Bubble max width.
    property real contentWidth: 0

    /// Long press on small controls passes to row menu.
    signal menuRequested()

    readonly property bool hasFile: root.filePath.length > 0
    // Sender names: `#`/`%` break file:// + path. Encode per segment (encodeURI keeps `#`, `?`).
    readonly property url fileUrl: root.hasFile
        ? Qt.resolvedUrl("file://" + root.filePath.split("/")
                                         .map(encodeURIComponent).join("/"))
        : ""

    readonly property bool isStill: root.viewType === "Image"
                                    || root.viewType === "Sticker"
    readonly property bool isAnimated: root.viewType === "Gif"
    readonly property bool isVideo: root.viewType === "Video"
    /// Video height/width: own within reason, else 16:9.
    readonly property real videoAspect:
        root.imageWidth > 0 && root.imageHeight > 0
        ? Math.min(root.imageHeight / root.imageWidth, 16 / 9) : 9 / 16
    readonly property bool isSound: root.viewType === "Audio"
                                    || root.viewType === "Voice"
    readonly property bool isCard: root.viewType === "Vcard"
    /// Sent app: tap runs it, not handed off.
    readonly property bool isApp: root.viewType === "Webxdc"
                                  && root.appsEnabled
    /// Picture kind.
    readonly property bool isPicture: root.isStill || root.isAnimated
    /// Fills bubble, not hugging text.
    readonly property bool wantsFullWidth: root.isPicture || root.isVideo
                                           || root.isSound || root.isCard
                                           || app.shown
    /// Fallback row text; bubble measures it.
    readonly property string genericText: generic.text
    /// Tap opens picture, video, file; sound plays in place. Press passes to row: long press
    /// reaches menu.
    readonly property bool openable: root.hasFile
                                     && (root.isPicture || root.isVideo || generic.shown)

    /// `Audio.PlayingState` as value: headless QtMultimedia stub is QML, can't declare enums on
    /// Qt 5.6; name reads `undefined` under test, button never turns to pause.
    readonly property int playingState: 1

    // One renderer built; read loaded item, not Loader (empty = 0). Animation inside still,
    // not summed.
    height: (still.item ? still.item.height : 0)
            + (video.item ? video.item.height : 0)
            + (sound.item ? sound.item.height : 0)
            + (card.item ? card.item.height : 0)
            + (app.item ? app.item.height : 0)
            + generic.height

    /// Readable size.
    function readableSize(bytes) {
        return Format.readableSize(bytes)
    }

    /// m:ss from ms.
    function clock(milliseconds) {
        var total = Math.floor(milliseconds / 1000)
        var seconds = total % 60
        return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    /// Picture height for `width`, from row's dimensions (core reads PNG/JPEG headers, model
    /// GIF's): right from start, rows below don't jump on decode. Decoded size wins only if
    /// shape disagrees (header pre-orientation: portrait stored landscape). Neither known:
    /// square (0 hides; PreserveAspectFit shows all).
    function pictureHeight(width, item) {
        var stored = root.imageWidth > 0 && root.imageHeight > 0
                     ? root.imageHeight / root.imageWidth : 0
        var decoded = item.implicitWidth > 0 && item.implicitHeight > 0
                      ? item.implicitHeight / item.implicitWidth : 0
        if (decoded > 0 && Math.abs(decoded - stored) > 0.02 * stored) {
            return width * decoded
        }
        if (stored > 0) {
            return width * stored
        }
        return width
    }

    /// Passes left: unseen GIF 3, older 0.
    property int runsLeft: 0
    readonly property int runsPerPlay: 3

    /// Play 3 passes.
    function replay() {
        root.runsLeft = root.runsPerPlay
    }

    /// Animated at all.
    readonly property bool animatable: root.isAnimated && root.hasFile

    // Type may arrive before or after file.
    onAnimatableChanged: if (root.animatable && root.isNew) root.replay()
    onIsNewChanged: if (root.animatable && root.isNew) root.replay()

    // GIF = image with animation over it: AnimatedImage has no size till decoded, draws nothing
    // on failure; Image always reads frame 1.
    Loader {
        id: still
        // Own `shown`, not `visible`: visibility drops while covered, so each return reloaded
        // mid-slide-in. See MessageDelegate, qml_hidden_rows.rs.
        readonly property bool shown: root.isPicture && root.hasFile
        active: still.shown
        sourceComponent: Component {
            Image {
                id: picture
                objectName: "attachmentImage"
                width: root.contentWidth
                height: root.pictureHeight(width, picture)
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                // Decode at drawn width (4 bytes/px; photos many screens wide): bounds memory,
                // crafted-file alloc. Width only: keeps shape.
                sourceSize.width: root.contentWidth
                // EXIF orientation honoured only with autoTransform: own photo lay sideways.
                autoTransform: true
                source: root.fileUrl

                // Movie only while passes left: stopped one holds decoded frames. Else still.
                AnimatedImage {
                    id: animated
                    objectName: "attachmentAnimation"
                    anchors.fill: parent
                    readonly property bool shown: root.animatable && root.runsLeft > 0
                    visible: animated.shown
                    fillMode: Image.PreserveAspectFit
                    // Decode per frame: Qt default caches whole movie.
                    cache: false
                    // Runs on screen only; file kept: no reload on return.
                    playing: animated.visible
                    // Held while list moves.
                    paused: root.scrolling
                    source: animated.shown ? root.fileUrl : ""

                    // Lower frame = wrapped. Counted, not clocked (per-frame delays).
                    property int lastFrame: 0
                    onSourceChanged: animated.lastFrame = 0
                    onFrameChanged: {
                        if (animated.currentFrame < animated.lastFrame) {
                            root.runsLeft -= 1
                        }
                        animated.lastFrame = animated.currentFrame
                    }
                    // Stopped when meant to run: pass done (or unstartable). Checked a pass later:
                    // movie reports inside own `source` load; `runsLeft` there changes `visible`.
                    onPlayingChanged: if (!animated.playing) stopped.restart()

                    Timer {
                        id: stopped
                        interval: 0
                        onTriggered: {
                            if (!animated.playing && animated.visible
                                    && animated.status === Image.Ready) {
                                root.runsLeft = 0
                            }
                        }
                    }
                }

                // Stopped GIF mark: tap plays here; long press to menu.
                Rectangle {
                    id: gifMark
                    objectName: "gifMark"
                    visible: root.animatable && !animated.shown
                    anchors.centerIn: parent
                    width: Theme.itemSizeSmall
                    height: width
                    radius: width / 2
                    color: Theme.rgba("black", 0.5)

                    Label {
                        anchors.centerIn: parent
                        color: "white"
                        font.pixelSize: Theme.fontSizeSmall
                        font.bold: true
                        textFormat: Text.PlainText
                        // Format name: untranslated.
                        text: "GIF"
                    }

                    MouseArea {
                        anchors.fill: parent
                        onClicked: root.replay()
                        onPressAndHold: root.menuRequested()
                    }
                }
            }
        }
    }

    // Video: poster with play mark; tap opens video page.
    Loader {
        id: video
        readonly property bool shown: root.isVideo && root.hasFile
        active: video.shown
        sourceComponent: Component {
            Item {
                objectName: "attachmentVideo"
                width: root.contentWidth
                // Own shape (sender's or track header), else 16:9; thumbnailer crops.
                height: Math.round(width * root.videoAspect)

                Rectangle {
                    anchors.fill: parent
                    radius: Theme.paddingSmall
                    color: Theme.rgba(Theme.highlightDimmerColor, 0.6)
                }

                Thumbnail {
                    id: poster
                    objectName: "videoThumbnail"
                    anchors.fill: parent
                    sourceSize.width: width
                    sourceSize.height: height
                    // Default fillMode crops; naming enum breaks headless stub.
                    mimeType: root.fileMime
                    source: root.fileUrl
                }

                // Frame is the control.
                Rectangle {
                    anchors.centerIn: parent
                    width: Theme.itemSizeSmall
                    height: width
                    radius: width / 2
                    color: Theme.rgba("black", 0.5)

                    Label {
                        anchors.centerIn: parent
                        color: "white"
                        font.pixelSize: Theme.fontSizeLarge
                        textFormat: Text.PlainText
                        text: "▶"
                    }
                }
            }
        }
    }

    // Sound played in place. Length: core's, else player's.
    Loader {
        id: sound
        readonly property bool shown: root.isSound && root.hasFile
        active: sound.shown
        sourceComponent: Component {
            Item {
                id: soundRow
                objectName: "attachmentAudio"
                width: root.contentWidth
                // Labels, track fit beside theme-sized button.
                height: Math.max(playButton.height,
                                 soundName.height + soundTime.height
                                 + track.height + 2 * Theme.paddingSmall)

                /// Player made on first play: on Sailfish each is a GStreamer pipeline paused on
                /// file at once (`autoLoad`), threads per scrolled-past sound otherwise.
                property bool wanted: false
                /// Player; null until then.
                readonly property QtObject player: playerLoader.item
                                                   ? playerLoader.item.player : null
                /// Sound length; 0 unknown.
                readonly property int duration:
                    soundRow.player && soundRow.player.duration > 0
                    ? soundRow.player.duration : root.durationMs
                readonly property int position: soundRow.player
                                                ? soundRow.player.position : 0
                readonly property bool playing: soundRow.player !== null
                    && soundRow.player.playbackState === root.playingState

                Loader {
                    id: playerLoader
                    active: soundRow.wanted
                    sourceComponent: Component {
                        // Item: Qt 5.6 Loader loads only Items.
                        Item {
                            objectName: "audioPlayer"
                            property alias player: audio

                            Audio {
                                id: audio
                                source: root.fileUrl
                                // Loaded by play press.
                                autoLoad: false
                                // No autoplay: talking in pocket.
                                autoPlay: false
                            }
                        }
                    }
                }

                IconButton {
                    id: playButton
                    objectName: "audioPlayButton"
                    anchors.verticalCenter: parent.verticalCenter
                    icon.source: soundRow.playing
                                 ? "image://theme/icon-m-pause"
                                 : "image://theme/icon-m-play"
                    onClicked: {
                        // First use; sync Loader builds on switch.
                        soundRow.wanted = true
                        var player = soundRow.player
                        if (!player) {
                            return
                        }
                        if (player.playbackState === root.playingState) {
                            player.pause()
                        } else {
                            player.play()
                        }
                    }
                    onPressAndHold: root.menuRequested()
                }

                Label {
                    id: soundName
                    objectName: "audioLabel"
                    anchors {
                        left: playButton.right
                        leftMargin: Theme.paddingSmall
                        right: parent.right
                        top: parent.top
                        topMargin: Theme.paddingSmall
                    }
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.primaryColor
                    textFormat: Text.PlainText
                    text: root.viewType === "Voice"
                          //: A recorded voice message, which has no useful file name.
                          ? qsTr("Voice message")
                          : root.fileName
                }

                // Position / duration once known; blank, not 0:00.
                Label {
                    id: soundTime
                    objectName: "audioTime"
                    anchors {
                        left: soundName.left
                        right: parent.right
                        top: soundName.bottom
                    }
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    textFormat: Text.PlainText
                    text: soundRow.duration > 0
                          ? root.clock(soundRow.position) + " / " + root.clock(soundRow.duration)
                          : ""
                }

                Rectangle {
                    id: track
                    anchors {
                        left: soundName.left
                        right: parent.right
                        bottom: parent.bottom
                        bottomMargin: Theme.paddingSmall
                    }
                    height: 2
                    color: Theme.rgba(Theme.secondaryColor, 0.3)

                    Rectangle {
                        objectName: "audioProgress"
                        height: parent.height
                        color: Theme.highlightColor
                        width: soundRow.duration > 0
                               ? parent.width * Math.min(1, soundRow.position / soundRow.duration)
                               : 0
                    }
                }
            }
        }
    }

    // Shared contact: core parses card (name, address, colour).
    Loader {
        id: card
        readonly property bool shown: root.isCard
        active: card.shown
        sourceComponent: Component {
            Item {
                objectName: "attachmentVcard"
                width: root.contentWidth
                height: Math.max(cardAvatar.height,
                                 cardName.height + cardAddress.height
                                 + 2 * Theme.paddingSmall)

                Avatar {
                    id: cardAvatar
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.itemSizeExtraSmall
                    height: width
                    initial: root.vcardName
                    ownColor: root.vcardColor
                }

                Label {
                    id: cardName
                    objectName: "vcardName"
                    anchors {
                        left: cardAvatar.right
                        leftMargin: Theme.paddingMedium
                        right: parent.right
                        top: parent.top
                        topMargin: Theme.paddingSmall
                    }
                    truncationMode: TruncationMode.Fade
                    color: Theme.primaryColor
                    // Card's name; emoji as Twemoji (Emoji.js).
                    // Label's own size (Silica default).
                    readonly property string pictured: Emoji.inText(root.vcardName, false,
                                                                    Theme.fontSizeMedium,
                                                                    color)
                    textFormat: pictured.length > 0 ? Text.StyledText : Text.PlainText
                    text: pictured.length > 0 ? pictured : root.vcardName
                }

                Label {
                    id: cardAddress
                    objectName: "vcardAddress"
                    anchors {
                        left: cardName.left
                        right: parent.right
                        top: cardName.bottom
                    }
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    textFormat: Text.PlainText
                    // Name often = address; twice reads as bug.
                    text: root.vcardAddr === root.vcardName ? "" : root.vcardAddr
                }
            }
        }
    }

    // webxdc app: core-read icon, name, status ("3 votes"). Tap runs it.
    Loader {
        id: app
        readonly property bool shown: root.isApp && root.webxdcName.length > 0
        active: app.shown
        sourceComponent: Component {
            Item {
                objectName: "attachmentApp"
                width: root.contentWidth
                height: Math.max(appIcon.height,
                                 appName.height + appState.height
                                 + 2 * Theme.paddingSmall)

                Rectangle {
                    id: appIcon
                    objectName: "webxdcIcon"
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.itemSizeSmall
                    height: width
                    radius: width / 8
                    color: Theme.rgba(Theme.highlightBackgroundColor,
                                      Theme.highlightBackgroundOpacity)

                    // No icon: mark, not blank square.
                    AppMark {
                        anchors.centerIn: parent
                        visible: root.webxdcIcon.length === 0
                        size: Theme.itemSizeSmall / 2
                    }

                    Image {
                        objectName: "webxdcIconImage"
                        anchors.fill: parent
                        anchors.margins: Theme.paddingSmall
                        fillMode: Image.PreserveAspectFit
                        asynchronous: true
                        // Segment-encoded, as all file URLs here.
                        source: root.webxdcIcon.length > 0
                                ? Qt.resolvedUrl("file://" + root.webxdcIcon.split("/")
                                                     .map(encodeURIComponent).join("/"))
                                : ""
                    }
                }

                Label {
                    id: appName
                    objectName: "webxdcName"
                    anchors {
                        left: appIcon.right
                        leftMargin: Theme.paddingMedium
                        right: parent.right
                        top: parent.top
                        topMargin: Theme.paddingSmall
                    }
                    truncationMode: TruncationMode.Fade
                    color: Theme.primaryColor
                    // App name is sender text.
                    textFormat: Text.PlainText
                    text: root.webxdcName
                }

                Label {
                    id: appState
                    objectName: "webxdcSummary"
                    anchors {
                        left: appName.left
                        right: parent.right
                        top: appName.bottom
                    }
                    // None when app says nothing.
                    readonly property bool shown: text.length > 0
                    visible: appState.shown
                    height: appState.shown ? implicitHeight : 0
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    textFormat: Text.PlainText
                    // Document it's editing, then its status.
                    text: {
                        var parts = []
                        if (root.webxdcDocument.length > 0) {
                            parts.push(root.webxdcDocument)
                        }
                        if (root.webxdcSummary.length > 0) {
                            parts.push(root.webxdcSummary)
                        }
                        return parts.join(" · ")
                    }
                }
            }
        }
    }

    // Any other file, undrawable webxdc too: named, sized, system on tap.
    Label {
        id: generic
        objectName: "attachmentLabel"
        readonly property bool shown: root.hasFile && !root.isStill
                                      && !root.isAnimated && !root.isVideo
                                      && !root.isSound && !root.isCard
                                      && !app.shown
        visible: generic.shown
        y: 0
        height: generic.shown ? implicitHeight : 0
        width: root.contentWidth
        truncationMode: TruncationMode.Fade
        color: Theme.highlightColor
        textFormat: Text.PlainText
        text: {
            // .vcf core won't class Vcard (no email, or >1 contact; phone-only common): still
            // marked as card. Unopenable .xdc: drawn as file.
            var mark = root.fileMime === "text/vcard" ? "📇" : "📎"
            var name = root.fileName.length > 0 ? root.fileName : root.filePath
            var size = root.readableSize(root.fileBytes)
            return mark + " " + name + (size.length > 0 ? " (" + size + ")" : "")
        }
    }
}
