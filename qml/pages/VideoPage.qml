import QtQuick 2.0
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import "../components"

/*
 * One video, played in-app (system player leaves Piirit and fails on device).
 * Same player as voice messages. Share-out in pull-down; copy via AttachmentSaver.
 */
Page {
    id: page

    /// File, encoded (AttachmentPreview.fileUrl).
    property url fileUrl
    property string fileName

    /// `MediaPlayer.PlayingState` value: enum lives on type, and headless
    /// QtMultimedia stub can't declare one on Qt 5.6. See AttachmentPreview.
    readonly property int playingState: 1

    readonly property bool playing: player.playbackState === page.playingState

    /// m:ss from ms.
    function clock(milliseconds) {
        var total = Math.floor(milliseconds / 1000)
        var seconds = total % 60
        return Math.floor(total / 60) + ":" + (seconds < 10 ? "0" : "") + seconds
    }

    function toggle() {
        if (page.playing) {
            player.pause()
        } else {
            player.play()
        }
    }

    Rectangle {
        anchors.fill: parent
        color: "black"
    }

    MediaPlayer {
        id: player
        objectName: "player"
        source: page.fileUrl
        autoPlay: true
    }

    // Copy the reader keeps: see AttachmentSaver.
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

    // Stop on way out: decoder must not run for a gone page.
    Component.onDestruction: player.stop()

    // Inside flickable (pull-down). `contentHeight` set: else children anchored to `parent` vanish.
    // Nothing scrolls. Stub can't test this layout.
    SilicaFlickable {
        anchors.fill: parent
        contentHeight: height

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
                // Off while copying (long for video): second pull = second copy.
                enabled: !saver.busy
                onClicked: saver.keep(page.fileUrl, page.fileName)
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
            // fillMode default; naming it needs an enum the stub lacks.
            source: player
            // Upright video stored sideways with turn in metadata; turned back,
            // as gallery does.
            autoOrientation: true

            MouseArea {
                anchors.fill: parent
                onClicked: page.toggle()
            }
        }

        // Where the copy went, or why not.
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
                icon.source: page.playing ? "image://theme/icon-m-pause"
                                          : "image://theme/icon-m-play"
                onClicked: page.toggle()
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
                text: page.clock(player.position) + " / " + page.clock(player.duration)
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
                // Never zero: equal ends leave no room for handle, and duration
                // is 0 until media loads.
                maximumValue: Math.max(1, player.duration)
                onReleased: player.seek(seek.value)
            }

            // Not a binding on `value`: dragging breaks it for good (stops
            // following after first seek). `when` restores.
            Binding {
                target: seek
                property: "value"
                value: player.position
                when: !seek.down
            }
        }
    }
}
