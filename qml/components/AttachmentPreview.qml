// 2.5: Image.autoTransform needs it for photo orientation. Harbour max 2.6.
import QtQuick 2.5
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0
import "../js/Format.js" as Format

/*
 * Renderer per attachment kind, from core's `viewType`. Nothing here inspects
 * files. Falls back to naming the file.
 *
 * Own component (MessageDelegate too long); testable per kind.
 * Bindings, not Column: positioner sizes in polish pass, never runs headless.
 * Every renderer at y 0; height is the loaded one. Each kind behind own Loader,
 * so a scrolled row builds only what it shows; sound player built only on first
 * play press. Nothing here starts on its own.
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
    /// Core kind enum: Text, Image, Gif, Sticker, Audio, Voice, Video, File,
    /// Call, Webxdc, Vcard.
    property string viewType: "Text"
    /// Pixel size; often 0 (core gave 0x0 for valid GIF): check before dividing.
    property int imageWidth: 0
    property int imageHeight: 0
    /// Reader has not seen it: arrived while chat open, or unread at open. Only
    /// such GIFs autoplay.
    property bool isNew: false
    /// Sound length ms when core knows (sender writes it for recordings; core
    /// reads none from a handed file), often 0. Shown until first play builds player.
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
    /// webxdc apps on. Off: `.xdc` drawn as file, opens to system. Page binds it.
    property bool appsEnabled: false
    /// Bubble max width.
    property real contentWidth: 0

    /// Long press on small controls passes to row menu.
    signal menuRequested()

    readonly property bool hasFile: root.filePath.length > 0
    // Sender-named file: `#` or `%` breaks plain file:// + path. Encode per
    // segment (encodeURI keeps `#`, `?`).
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
    /// Tap opens picture, video, or file. Sound plays in place; contact drawn.
    /// Press passes to row, so long press reaches menu.
    readonly property bool openable: root.hasFile
                                     && (root.isPicture || root.isVideo || generic.shown)

    /// `Audio.PlayingState` written as its value. Enum lives on the type; the
    /// headless QtMultimedia stub is a QML component, which can't declare enums on
    /// Qt 5.6. `Audio.PlayingState` reads `undefined` under test, never matches:
    /// play button silently never turns to pause.
    readonly property int playingState: 1

    // One renderer built; read loaded item, not Loader (empty = zero). Animation
    // sits in still image, not summed.
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

    /// Picture height for `width`.
    ///
    /// From row's given dimensions (core reads PNG/JPEG headers, model GIF's):
    /// right height from start, so rows below don't jump as pictures decode.
    /// Decoded size wins only where shape disagrees (header is pre-orientation:
    /// portrait photo stored landscape); same shape within rounding stays put.
    /// Square when neither known: height 0 hides it, PreserveAspectFit shows all.
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

    /// Animation runs left; run = one pass. Unseen GIF: 3. Older GIF: no autoplay.
    property int runsLeft: 0
    readonly property int runsPerPlay: 3

    /// Play animation, 3 runs.
    function replay() {
        root.runsLeft = root.runsPerPlay
    }

    /// Animation here at all.
    readonly property bool animatable: root.isAnimated && root.hasFile

    // Row learns type in either order: placeholder, or message before file.
    onAnimatableChanged: if (root.animatable && root.isNew) root.replay()
    onIsNewChanged: if (root.animatable && root.isNew) root.replay()

    // Every picture: GIF = image with animation over it. AnimatedImage has no
    // implicit size until decoded, and draws nothing on decode failure. Image
    // reads first frame always.
    Loader {
        id: still
        // Built by own `shown`, not `visible`: effective visibility drops while
        // another page covers this one, so every return reloaded it mid-slide-in.
        // See MessageDelegate, qml_hidden_rows.rs.
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
                // Decoded at drawn width: texture 4 bytes/px, photos many screen
                // widths. Bounds memory and crafted-file alloc. Width only, so
                // height keeps picture's shape.
                sourceSize.width: root.contentWidth
                // Camera writes orientation to EXIF; other clients honour it, Image
                // only with autoTransform: own sent photo lay sideways here.
                autoTransform: true
                source: root.fileUrl

                // Movie only while runs left: stopped movie holds decoded frames
                // (memory). Else still shows frame 1.
                AnimatedImage {
                    id: animated
                    objectName: "attachmentAnimation"
                    anchors.fill: parent
                    readonly property bool shown: root.animatable && root.runsLeft > 0
                    visible: animated.shown
                    fillMode: Image.PreserveAspectFit
                    // Frame decoded on demand: Qt default caches whole movie.
                    cache: false
                    // Runs only while on screen; file kept so return is no reload.
                    playing: animated.visible
                    // Held on frame while list moves under finger.
                    paused: root.scrolling
                    source: animated.shown ? root.fileUrl : ""

                    // Frame smaller than last = movie wrapped. Counted, not clocked
                    // (frames have own delay).
                    property int lastFrame: 0
                    onSourceChanged: animated.lastFrame = 0
                    onFrameChanged: {
                        if (animated.currentFrame < animated.lastFrame) {
                            root.runsLeft -= 1
                        }
                        animated.lastFrame = animated.currentFrame
                    }
                    // Not running when meant to: run done (or unstartable). Checked a pass
                    // later: movie reports from inside its own `source` load, and
                    // changing `runsLeft` there changes `visible`.
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

                // Mark on stopped GIF: own tap plays here; long press to menu.
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
                        // Format name, same in every language.
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

    // Video: poster with play mark. Tap opens video page; nothing takes press.
    Loader {
        id: video
        readonly property bool shown: root.isVideo && root.hasFile
        active: video.shown
        sourceComponent: Component {
            Item {
                objectName: "attachmentVideo"
                width: root.contentWidth
                // Video's own shape (sender's or track header), else 16:9.
                // Thumbnailer crops to size.
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
                    // fillMode default crops; naming enum breaks headless stub.
                    mimeType: root.fileMime
                    source: root.fileUrl
                }

                // Over frame: frame is the control.
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

    // Voice or music, played in place. Length: core's if present, else player's.
    Loader {
        id: sound
        readonly property bool shown: root.isSound && root.hasFile
        active: sound.shown
        sourceComponent: Component {
            Item {
                id: soundRow
                objectName: "attachmentAudio"
                width: root.contentWidth
                // Measured: labels and track fit beside theme-sized button.
                height: Math.max(playButton.height,
                                 soundName.height + soundTime.height
                                 + track.height + 2 * Theme.paddingSmall)

                /// Player made on first play press. On Sailfish a player is a
                /// GStreamer pipeline, paused on the file at once (default
                /// `autoLoad`): streaming threads per scrolled-past sound otherwise.
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
                        // In an Item: Qt 5.6 Loader loads only Items.
                        Item {
                            objectName: "audioPlayer"
                            property alias player: audio

                            Audio {
                                id: audio
                                source: root.fileUrl
                                // Loaded by the play press, not creation.
                                autoLoad: false
                                // No autoplay: talking on scroll bad in pocket.
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
                        // Made on first use; non-async Loader builds on switch.
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

                // Position / duration once known. Blank, not 0:00.
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

    // Shared contact: core parses card (name, address, colour). No vCard here.
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
                    textFormat: Text.PlainText
                    text: root.vcardName
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
                    // Name often equals address; saying twice reads as bug.
                    text: root.vcardAddr === root.vcardName ? "" : root.vcardAddr
                }
            }
        }
    }

    // webxdc app: icon, name, status line ("3 votes"), core-read. Tap runs it.
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

                    // App icon; mark when none, never blank square.
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
                        // Segment-encoded, like every file URL here.
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
                    // No status line when app says nothing.
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

    // Everything else with a file: named, sized, system on tap. Undrawable webxdc
    // lands here.
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
            // .vcf core won't class Vcard (no email, or >1 contact; phone-only
            // contacts are common, unchattable): still marked as a card, not a
            // mystery blob. .xdc core won't open: drawn as the file it is.
            var mark = root.fileMime === "text/vcard" ? "📇" : "📎"
            var name = root.fileName.length > 0 ? root.fileName : root.filePath
            var size = root.readableSize(root.fileBytes)
            return mark + " " + name + (size.length > 0 ? " (" + size + ")" : "")
        }
    }
}
