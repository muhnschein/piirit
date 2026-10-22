import QtQuick 2.0
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0

/*
 * One video, played in-app (system player leaves Piirit, fails on device): GalleryPage's video
 * slide, as Jolla Gallery's (VideoPoster): platform's still frame, play button over it; plays
 * on its tap, not on swipe-in. Seek bar, 10 s steps, Save, Share: page's ViewerOverlay.
 */
Item {
    id: root

    /// File, encoded (AttachmentPreview.fileUrl).
    property url fileUrl
    property string fileName
    property string mimeType: "video/mp4"
    /// On screen, swipe over, page in place. Off: paused (cheap; mid-swipe).
    property bool active: true
    /// Player keeps file. Off: stopped, file out (no decoder per neighbour; swiping back
    /// starts over). Gallery drops it only once swipe over: pipeline teardown is main-thread
    /// work that stalled the swipe's frames.
    property bool kept: true
    /// Page's controls up: play button stays while playing too (pause), as Jolla's.
    property bool controlsShown: true
    /// Length, ms, as sender wrote it (row's); 0 unknown. Till player has file: mid-swipe it
    /// has none, and Qt 5.6 says -1 ("-1:0-1").
    property int durationMs: 0
    /// Player's length once it knows, else sender's; 0 unknown.
    readonly property int length:
        player.duration > 0 ? player.duration : Math.max(0, root.durationMs)
    /// Where playback is, ms; Qt 5.6 says -1 too.
    readonly property int position: Math.max(0, player.position)

    /// `MediaPlayer.PlayingState` value: enum lives on type; headless QtMultimedia stub can't
    /// declare one on Qt 5.6. See AttachmentPreview.
    readonly property int playingState: 1

    readonly property bool playing: player.playbackState === root.playingState

    /// Seek asked before player knew length: done once it does.
    property int pendingSeek: -1

    /// Single tap (no second follows): page shows or hides its controls.
    signal tapped()

    /// m:ss from ms.
    function clock(milliseconds) {
        var total = Math.floor(milliseconds / 1000)
        var seconds = total % 60
        return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    /// File into player, once: frame drawn, seekable.
    function load() {
        if (String(player.source).length === 0 && String(root.fileUrl).length > 0) {
            player.source = root.fileUrl
        }
    }

    function toggle() {
        if (root.playing) {
            player.pause()
        } else {
            root.load()
            player.play()
        }
    }

    function pause() {
        player.pause()
    }

    /// To `milliseconds`, kept inside video. Unloaded: file in, paused, seek once length known.
    function seekTo(milliseconds) {
        var target = Math.max(0, milliseconds)
        root.load()
        if (player.duration > 0) {
            player.seek(Math.min(target, player.duration))
        } else {
            root.pendingSeek = target
            player.pause()
        }
    }

    function seekBy(milliseconds) {
        root.seekTo(root.position + milliseconds)
    }

    /// Back to poster: stopped, file out.
    function reset() {
        root.pendingSeek = -1
        player.stop()
        player.source = ""
    }

    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    MediaPlayer {
        id: player
        objectName: "player"
        onDurationChanged: {
            if (player.duration > 0 && root.pendingSeek >= 0) {
                player.seek(Math.min(root.pendingSeek, player.duration))
                root.pendingSeek = -1
            }
        }
    }

    onActiveChanged: if (!root.active && root.playing) player.pause()
    onKeptChanged: if (!root.kept) root.reset()
    onFileUrlChanged: root.reset()

    // Stop on way out: no decoder for gone slide.
    Component.onDestruction: player.stop()

    // Platform thumbnailer's frame (grid's, cached) until player has file: shows video, not
    // black.
    Thumbnail {
        objectName: "poster"
        anchors.fill: parent
        visible: String(player.source).length === 0
        sourceSize.width: width
        sourceSize.height: height
        // Whole frame, as player draws it.
        fillMode: Thumbnail.PreserveAspectFit
        mimeType: root.mimeType
        source: visible ? root.fileUrl : ""
    }

    VideoOutput {
        objectName: "videoOutput"
        anchors.fill: parent
        // fillMode default; naming it needs enum stub lacks.
        source: player
        // Not `autoOrientation`: turns with screen, and page turns too: twice, sideways or
        // distorted.
    }

    // Not near edges (Silica's swipes start there). Double tap: 10 s on, as Jolla's.
    MouseArea {
        id: tap
        objectName: "videoTap"
        anchors.fill: parent
        anchors.margins: Theme.paddingLarge
        onClicked: single.restart()
        onDoubleClicked: {
            single.stop()
            root.seekBy(10000)
        }
    }

    // First tap waits out double-tap time: seeking must not flash controls.
    Timer {
        id: single
        interval: Qt.styleHints.mouseDoubleClickInterval
        onTriggered: root.tapped()
    }

    IconButton {
        objectName: "playButton"
        anchors.centerIn: parent
        // Playing with controls away: nothing over picture.
        enabled: root.controlsShown || !root.playing
        opacity: enabled ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 200 } }
        icon.source: "image://theme/icon-video-overlay-" + (root.playing ? "pause" : "play")
        onClicked: root.toggle()
    }
}
