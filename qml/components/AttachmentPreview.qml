// 2.5 rather than 2.0 for Image.autoTransform, which is what reads a
// photo's orientation tag. Harbour allows QtQuick up to 2.6.
import QtQuick 2.5
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0
import "../js/Format.js" as Format

/*
 * What a message's attachment looks like, whatever kind it is.
 *
 * The core classifies every attachment from the file itself and hands back
 * a `viewType`; nothing here inspects a file, and nothing should start.
 * This picks a renderer from that answer, and falls back to naming the file
 * for the kinds nothing better can be done with.
 *
 * Its own component because MessageDelegate is already the longest file in
 * the tree, and because a preview that can be loaded on its own is one a
 * test can drive through every kind in turn.
 *
 * Laid out by bindings rather than a Column, for the same reason
 * MessageDelegate is: a positioner sizes itself in a polish pass, which
 * never runs headlessly, so a row built from one cannot be measured. Every
 * renderer sits at y 0, and the height is the one that is there.
 *
 * Each kind's renderer is behind a Loader of its own, active only for that
 * kind. A row is built for every message scrolled past, and a picture row
 * that also built a sound player, a poster, a contact card and an app card
 * -- all of them unseen -- was about forty objects more than it draws, and
 * the sound player alone a media pipeline. The sound player is a step
 * further back still: it is made on the first press of play.
 */
Item {
    id: root

    /// Absolute path to the file in the core's blob directory.
    property string filePath: ""
    /// What to call it.
    property string fileName: ""
    /// The core's MIME type for it, empty when it has none.
    property string fileMime: ""
    /// Size in bytes, 0 when unknown. A real, not an int: QML has no
    /// 64-bit integer and a large file overflows one.
    property real fileBytes: 0
    /// Text, Image, Gif, Sticker, Audio, Voice, Video, File, Call, Webxdc,
    /// Vcard -- the core's own enum.
    property string viewType: "Text"
    /// Pixel size when the core knows it. Often 0: it reads PNG and JPEG
    /// but returned 0x0 for a valid GIF, so nothing here may divide by
    /// these without checking.
    property int imageWidth: 0
    property int imageHeight: 0
    /// True for a message the reader has not seen before: one that came in
    /// while the chat was open, or was unread when it was opened. The only
    /// GIFs that play by themselves.
    property bool isNew: false
    /// How long a sound is, in milliseconds, when the core knows: the
    /// sending client writes it in for a recording, and the core reads
    /// nothing out of a file it is merely handed, so it is often 0. What
    /// the row says before the player has been made, which is until the
    /// reader first presses play.
    property int durationMs: 0
    /// The list this sits in is being dragged or flicked. A GIF holds its
    /// frame meanwhile rather than decoding the next on the thread the
    /// scrolling is drawn on.
    property bool scrolling: false
    /// A shared contact, for `Vcard`.
    property string vcardName: ""
    property string vcardAddr: ""
    property string vcardColor: ""
    /// A webxdc app, for `Webxdc`: what the core reads out of the app
    /// itself. The name is what says the app could be read at all; a
    /// .xdc the core would not open has none, and draws as a file.
    property string webxdcName: ""
    property string webxdcDocument: ""
    property string webxdcSummary: ""
    property string webxdcIcon: ""
    /// Whether webxdc apps are on. Off, a `.xdc` is drawn as the file it
    /// is and handed to the system on a tap -- the same row the core's
    /// own refusal to read one already lands on -- rather than as an app
    /// with a tap that would run it. The page binds it from the reader's
    /// setting, as it does the Markdown mode.
    property bool appsEnabled: false
    /// How wide the bubble lets this be.
    property real contentWidth: 0

    /// A long press landed on one of the small controls here -- the
    /// play button, the mark on a GIF -- which take the press for
    /// themselves. The row's menu is what a long press means anywhere on
    /// a message, so the control passes it on rather than swallowing it.
    signal menuRequested()

    readonly property bool hasFile: root.filePath.length > 0
    // Encoded, not concatenated: attachments are named by whoever sent
    // them, and a "#" or a "%" in the name makes a plain "file://" + path
    // into a URL that points somewhere else, or nowhere. Per segment,
    // with encodeURIComponent: encodeURI leaves "#" and "?" alone as
    // URL syntax, which is exactly what they must not be here.
    readonly property url fileUrl: root.hasFile
        ? Qt.resolvedUrl("file://" + root.filePath.split("/")
                                         .map(encodeURIComponent).join("/"))
        : ""

    readonly property bool isStill: root.viewType === "Image"
                                    || root.viewType === "Sticker"
    readonly property bool isAnimated: root.viewType === "Gif"
    readonly property bool isVideo: root.viewType === "Video"
    /// Height over width for a video: its own, within reason, or 16:9.
    readonly property real videoAspect:
        root.imageWidth > 0 && root.imageHeight > 0
        ? Math.min(root.imageHeight / root.imageWidth, 16 / 9) : 9 / 16
    readonly property bool isSound: root.viewType === "Audio"
                                    || root.viewType === "Voice"
    readonly property bool isCard: root.viewType === "Vcard"
    /// An app somebody sent, where apps are on. A tap on one runs it
    /// rather than handing it to another app, so the row says so whether
    /// or not the core could read enough of it to draw a card.
    readonly property bool isApp: root.viewType === "Webxdc"
                                  && root.appsEnabled
    /// A picture of some kind.
    readonly property bool isPicture: root.isStill || root.isAnimated
    /// True when this kind reads better filling the bubble than hugging
    /// its own text, which is what the bubble sizes itself by otherwise.
    readonly property bool wantsFullWidth: root.isPicture || root.isVideo
                                           || root.isSound || root.isCard
                                           || app.shown
    /// The text the fallback row shows, so the bubble can measure it
    /// without reaching inside here for the label.
    readonly property string genericText: generic.text
    /// Whether a tap on the message opens this: a picture, a video, or a
    /// file for another app. A sound plays where it sits, and a shared
    /// contact is drawn rather than opened. The tap itself is the row's:
    /// nothing here takes a press, so a long press anywhere on the
    /// message -- the picture included -- reaches the row's menu.
    readonly property bool openable: root.hasFile
                                     && (root.isPicture || root.isVideo || generic.shown)

    /// `Audio.PlayingState`, written as its value.
    ///
    /// The enum lives on the type rather than the instance, and the
    /// headless stub that stands in for QtMultimedia is a QML component,
    /// which cannot declare one on Qt 5.6. Comparing against
    /// `Audio.PlayingState` therefore reads as `undefined` under test and
    /// silently never matches -- the play button would simply never become
    /// a pause button, and nothing would fail.
    readonly property int playingState: 1

    // Only one renderer is ever built, and the rest are not there to
    // measure. Read off the loaded item rather than the Loader, so one
    // with nothing in it is zero by construction rather than by whatever
    // size it was last given. The animation is
    // inside the still image rather than beside it, so it is not in this
    // sum.
    height: (still.item ? still.item.height : 0)
            + (video.item ? video.item.height : 0)
            + (sound.item ? sound.item.height : 0)
            + (card.item ? card.item.height : 0)
            + (app.item ? app.item.height : 0)
            + generic.height

    /// A file size a person can read: Format.readableSize, reachable on
    /// the preview because the fallback row is where a size is shown.
    function readableSize(bytes) {
        return Format.readableSize(bytes)
    }

    /// m:ss from milliseconds, which is what QtMultimedia reports.
    function clock(milliseconds) {
        var total = Math.floor(milliseconds / 1000)
        var seconds = total % 60
        return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    /// How tall a picture `width` wide should be.
    ///
    /// From the dimensions the row was handed, before the picture has
    /// been decoded and after: a row that is the right height from the
    /// start does not change height when the picture lands, and a row
    /// that changes height moves every row below it, which under a
    /// reader scrolling through a chat full of pictures is the list
    /// jumping about as they decode. The core reads PNG and JPEG
    /// headers and the model reads a GIF's, so the dimensions are nearly
    /// always there.
    ///
    /// The decoded picture wins only where it disagrees. The header's
    /// dimensions are the stored ones, before any turn the orientation
    /// tag asks for: a photo taken in portrait is stored landscape and
    /// marked, and a box shaped from the stored size is the wrong way
    /// round for it. Same shape, to within rounding, means the header
    /// was right and the row stays put.
    ///
    /// A square is the fallback when neither is known, because a height
    /// of zero is an attachment the reader cannot see at all, and
    /// `PreserveAspectFit` shows the whole picture in a square box
    /// whatever shape it turns out to be.
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

    /// How many runs of the animation are left to play. A run is one pass
    /// through the frames. Three of them is what a GIF the reader has not
    /// seen before gets, and what the mark on a stopped one buys; a GIF
    /// from last week does not start on its own.
    property int runsLeft: 0
    readonly property int runsPerPlay: 3

    /// Play the animation, three runs' worth.
    function replay() {
        root.runsLeft = root.runsPerPlay
    }

    /// Whether there is an animation here to play at all.
    readonly property bool animatable: root.isAnimated && root.hasFile

    // Both, because the row learns what it is in either order: a
    // placeholder is filled in as a GIF that is already new, or a message
    // arrives and is new before its file is there.
    onAnimatableChanged: if (root.animatable && root.isNew) root.replay()
    onIsNewChanged: if (root.animatable && root.isNew) root.replay()

    // Every picture, animated or not. A GIF is this image with the
    // animation laid over it, rather than a second renderer beside it, for
    // two reasons that both come down to AnimatedImage being unreliable
    // about what it is holding: it reports no implicit size until its
    // movie has decoded a frame -- headlessly it never does -- and if the
    // decode fails it draws nothing at all. An Image reads the first frame
    // of a GIF regardless. So the Image sizes the row and shows the still,
    // and the animation covers it when there is one to play.
    Loader {
        id: still
        // Each renderer is built, and loads its file, by its own reason
        // to be here rather than by `visible`: that reads the effective
        // visibility, which the platform takes away from the whole page
        // while another is over it -- and a picture, a poster or a sound
        // player that unloaded on every such hiding was loaded again on
        // every return, while the page was still sliding in. See
        // MessageDelegate, and qml_hidden_rows.rs.
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
                // Decoded no wider than it is drawn. A picture is kept as
                // a texture of four bytes a pixel for as long as its row
                // is built, and a phone's own photo is several times the
                // width of its screen; this is most of what a chat full
                // of pictures costs in memory, and the bound on what a
                // crafted file can make the app allocate. Only the width,
                // so the height follows the picture's own shape.
                sourceSize.width: root.contentWidth
                // A camera reads its sensor out landscape however the
                // phone is held and writes which way to turn it into an
                // EXIF tag rather than into the pixels. Every other client
                // honours the tag; Image does not unless it is asked,
                // which is why a photo sent from here arrived upright for
                // whoever received it and lay on its side in our own
                // message view.
                autoTransform: true
                source: root.fileUrl

                // The movie, while there are runs left to play, and
                // nothing at all otherwise: a stopped movie still holds
                // every frame it decoded, and a long conversation of GIFs
                // holding theirs is the memory a phone notices. Unloaded,
                // the still underneath shows its first frame.
                AnimatedImage {
                    id: animated
                    objectName: "attachmentAnimation"
                    anchors.fill: parent
                    readonly property bool shown: root.animatable && root.runsLeft > 0
                    visible: animated.shown
                    fillMode: Image.PreserveAspectFit
                    // Each frame decoded as it comes round rather than
                    // every frame kept: Qt's default caches the whole
                    // movie at full size, which for a GIF of a hundred
                    // frames is tens of megabytes held for one row.
                    cache: false
                    // Runs only while actually on screen -- `visible` is
                    // the effective one, and a page hidden under another
                    // has no business decoding frames -- but keeps its
                    // file by its own reason to be here, so coming back is
                    // not a reload.
                    playing: animated.visible
                    // Held on its frame while the list moves under the
                    // reader's finger, and carried on from there.
                    paused: root.scrolling
                    source: animated.shown ? root.fileUrl : ""

                    // The frame before this one: a smaller number now is
                    // the movie having gone round. Counted here rather
                    // than from a clock, because a GIF's frames each carry
                    // their own delay.
                    property int lastFrame: 0
                    onSourceChanged: animated.lastFrame = 0
                    onFrameChanged: {
                        if (animated.currentFrame < animated.lastFrame) {
                            root.runsLeft -= 1
                        }
                        animated.lastFrame = animated.currentFrame
                    }
                    // A movie that is not running while it is meant to be
                    // has had its run: a GIF made to play once, or a file
                    // the movie reader could not start on at all, which
                    // the still shows as well as anything could. Asked a
                    // pass later rather than in the handler: the movie
                    // reports this from inside the load its own `source`
                    // binding started, and changing `runsLeft` there is
                    // changing what `visible` is being worked out from.
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

                // The mark on a GIF that is not playing. Its own tap, so a
                // tap on the mark plays the movie here rather than opening
                // the picture; a long press on it still goes to the row's
                // menu.
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
                        // The format's name, which is the same in every
                        // language.
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

    // A video: the platform thumbnailer's poster frame, with a play mark
    // over it. A tap on the row opens it on the video page; nothing here
    // takes the press.
    Loader {
        id: video
        readonly property bool shown: root.isVideo && root.hasFile
        active: video.shown
        sourceComponent: Component {
            Item {
                objectName: "attachmentVideo"
                width: root.contentWidth
                // The video's own shape where it is known -- what the
                // sender wrote in, or the file's track header read by the
                // model -- so a video taken upright stands upright; 16:9
                // where it is not. The thumbnailer crops to whatever it is
                // given.
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
                    // fillMode left at the thumbnailer's own default,
                    // which crops to the size asked for. Naming a value
                    // would mean naming an enum on the type, which the
                    // headless stub cannot carry.
                    mimeType: root.fileMime
                    source: root.fileUrl
                }

                // Over the frame rather than beside it: the frame is the
                // control.
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

    // A voice message or a music file, played where it sits. The length
    // is the core's when it has one, which it has for a recording whose
    // sender wrote it in, and the player's own once there is a player:
    // the core reads none out of a file it is merely handed.
    Loader {
        id: sound
        readonly property bool shown: root.isSound && root.hasFile
        active: sound.shown
        sourceComponent: Component {
            Item {
                id: soundRow
                objectName: "attachmentAudio"
                width: root.contentWidth
                // Measured rather than fixed: the two labels and the track
                // have to fit beside a button whose size is the theme's,
                // not ours.
                height: Math.max(playButton.height,
                                 soundName.height + soundTime.height
                                 + track.height + 2 * Theme.paddingSmall)

                /// The reader has pressed play on this row: the player is
                /// made then and not before. On Sailfish a player is a
                /// GStreamer pipeline, made when the player is and, with
                /// the default `autoLoad`, taken as far as paused on the
                /// file straight away -- streaming threads and an audio
                /// stream, for every sound scrolled past.
                property bool wanted: false
                /// The player, null until then.
                readonly property QtObject player: playerLoader.item
                                                   ? playerLoader.item.player : null
                /// How long the sound is, 0 while nobody knows.
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
                        // In an Item because a Loader on Qt 5.6 loads
                        // nothing else, and Audio is not one.
                        Item {
                            objectName: "audioPlayer"
                            property alias player: audio

                            Audio {
                                id: audio
                                source: root.fileUrl
                                // Loaded by the press of play that made
                                // it, and not by being made.
                                autoLoad: false
                                // Nothing autoplays. A conversation that
                                // starts talking when it is scrolled past
                                // is the worst possible behaviour on a
                                // phone that may be in a pocket.
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
                        // Made here, the first time, and there by the
                        // next line: a Loader that is not asynchronous
                        // builds as it is switched on.
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

                // Position over duration, once there is a duration to be
                // over. Blank rather than "0:00" while nobody knows: a
                // length of zero is a claim, and an empty label is not.
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

    // A shared contact. The core parses the card and hands back the name,
    // the address and the colour it would give that contact; nothing here
    // reads vCard syntax.
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
                    // The card usually carries the address as the name
                    // too, and saying it twice reads as a bug rather than
                    // as thoroughness.
                    text: root.vcardAddr === root.vcardName ? "" : root.vcardAddr
                }
            }
        }
    }

    // A webxdc app: its own icon, its name, and the line it keeps about
    // itself -- "3 votes", a score -- which changes as the chat plays
    // with it. All three are the core's, read out of the app; a tap runs
    // it (MessageDelegate).
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

                    // The app's own icon when it has one. The mark stands
                    // in where it has not, so the row is never a blank
                    // square.
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
                        // Encoded a segment at a time, as every file URL
                        // here is.
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
                    // The app named itself; it is the sender's text like
                    // any other.
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
                    // An app that has said nothing about itself takes no
                    // line for saying so.
                    readonly property bool shown: text.length > 0
                    visible: appState.shown
                    height: appState.shown ? implicitHeight : 0
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    textFormat: Text.PlainText
                    // The document it is editing, then what it says about
                    // itself.
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

    // Everything else that has a file: named, sized, and handed to the
    // system on a tap on the row. A webxdc the core could not read enough
    // of to draw as an app lands here too, marked as one.
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
            // A .vcf the core would not call a Vcard still says what
            // it is. It declines the classification when the card
            // holds no email address or more than one contact -- a
            // phone-only contact exported from the address book is
            // both common and not someone Delta Chat can open a chat
            // with -- and a paperclip makes that look like a mystery
            // blob rather than a contact this app cannot use.
            //
            // A .xdc that got this far is one the core would not open
            // as an app, so it is drawn as the file it turned out to be
            // rather than as an app that does not work.
            var mark = root.fileMime === "text/vcard" ? "📇" : "📎"
            var name = root.fileName.length > 0 ? root.fileName : root.filePath
            var size = root.readableSize(root.fileBytes)
            return mark + " " + name + (size.length > 0 ? " (" + size + ")" : "")
        }
    }
}
