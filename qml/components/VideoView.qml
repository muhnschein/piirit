import QtQuick 2.0
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import "."

/*
 * One video, played in-app (system player leaves Piirit, fails on device): GalleryPage's
 * video slide. Share-out in pull-down; copy via AttachmentSaver.
 */
Item {
    id: root

    /// File, encoded (AttachmentPreview.fileUrl).
    property url fileUrl
    property string fileName
    /// Slide on screen, page in place. Off: no file in player (no decoder per neighbour),
    /// so swiping away stops, back starts over.
    property bool active: true

    /// `MediaPlayer.PlayingState` value: enum lives on type; headless QtMultimedia stub can't
    /// declare one on Qt 5.6. See AttachmentPreview.
    readonly property int playingState: 1

    readonly property bool playing: player.playbackState === root.playingState

    /// m:ss from ms.
    function clock(milliseconds) {
        var total = Math.floor(milliseconds / 1000)
        var seconds = total % 60
        return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    function toggle() {
        if (root.playing) {
            player.pause()
        } else {
            player.play()
        }
    }

    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    // File handed over by `follow`, not bound: binding and play order would race.
    MediaPlayer {
        id: player
        objectName: "player"
    }

    /// Active: file in, playing. Off: stopped, file out.
    function follow() {
        if (root.active && String(root.fileUrl).length > 0) {
            player.source = root.fileUrl
            player.play()
        } else {
            player.stop()
            player.source = ""
        }
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

    // Plays on open, on swipe to; stops on swipe away.
    onActiveChanged: root.follow()
    onFileUrlChanged: root.follow()
    Component.onCompleted: root.follow()

    // Stop on way out: no decoder for gone slide.
    Component.onDestruction: player.stop()

    // In flickable (pull-down). `contentHeight` set, else `parent`-anchored children vanish.
    // Nothing scrolls. Stub can't test layout.
    SilicaFlickable {
        anchors.fill: parent
        contentHeight: height

        PullDownMenu {
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
                // Off while copying (long for video): second pull = second copy.
                enabled: !saver.busy
                onClicked: saver.keep(root.fileUrl, root.fileName)
            }
        }

        VideoOutput {
            objectName: "videoOutput"
            anchors {
                left: parent.left
                right: parent.right
                top: notice.bottom
                bottom: controls.top
            }
            // fillMode default; naming it needs enum stub lacks.
            source: player
            // Upright video stored sideways with turn in metadata; turned back, as gallery.
            autoOrientation: true

            MouseArea {
                anchors.fill: parent
                onClicked: root.toggle()
            }
        }

        // Where copy went, or why not.
        Banner {
            id: notice
            objectName: "notice"
            labelObjectName: "noticeLabel"
            tone: "info"
            timeout: 4
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
            }
            onDismissed: notice.text = ""
        }

        Item {
            id: controls
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            height: Theme.itemSizeLarge

            IconButton {
                id: playButton
                objectName: "playButton"
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                icon.source: root.playing ? "image://theme/icon-m-pause"
                                          : "image://theme/icon-m-play"
                onClicked: root.toggle()
            }

            Label {
                id: elapsed
                objectName: "elapsed"
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                text: root.clock(player.position) + " / " + root.clock(player.duration)
            }

            Slider {
                id: seek
                objectName: "seek"
                anchors {
                    left: playButton.right
                    right: elapsed.left
                    verticalCenter: parent.verticalCenter
                }
                minimumValue: 0
                // Never zero: equal ends leave no handle room; duration 0 until loaded.
                maximumValue: Math.max(1, player.duration)
                onReleased: player.seek(seek.value)
            }

            // Not bound to `value`: drag breaks binding for good (stops following after first
            // seek). `when` restores.
            Binding {
                target: seek
                property: "value"
                value: player.position
                when: !seek.down
            }
        }
    }
}
