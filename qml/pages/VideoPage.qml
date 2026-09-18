import QtQuick 2.0
import QtMultimedia 5.6
import Sailfish.Silica 1.0
import "../components"
import Piiri 1.0

/*
 * One video, played here.
 *
 * Played here rather than handed to whatever the system thinks plays
 * video, which leaves Piiri and fails on a device. QtMultimedia is
 * already how a voice message plays in its own row; this is the same
 * player with a picture. The way out to another app stays in the pull-down,
 * and so does a copy into the Videos folder, where the gallery finds it.
 */
Page {
    id: page

    /// The file, already encoded: see AttachmentPreview.fileUrl.
    property url fileUrl
    property string fileName

    /// `MediaPlayer.PlayingState`, written as its value.
    ///
    /// The enum lives on the type, and the headless stub that stands in for
    /// QtMultimedia is a QML component, which cannot declare one on Qt 5.6.
    /// See AttachmentPreview for the same problem and the same answer.
    readonly property int playingState: 1

    readonly property bool playing: player.playbackState === page.playingState

    /// m:ss from milliseconds, which is what QtMultimedia reports.
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

    // A copy for the gallery. The folder is the platform's own answer to
    // where videos go; the sandbox grants it (UserDirs).
    FileSaver {
        id: saver
        objectName: "saver"
        onSaved: {
            notice.tone = "info"
            notice.show(qsTr("Saved to Videos"))
        }
        onError: {
            notice.tone = "error"
            notice.show(message)
        }
    }

    // Stopped on the way out. A page that is gone is not a reason for a
    // decoder to keep running, and on a phone it is a reason it should not.
    Component.onDestruction: player.stop()

    // Everything is inside the flickable because the pull-down has to be,
    // and `contentHeight` is set because a flickable leaves it at 0: a
    // child anchored to `parent` inside one is anchored to a content item
    // with no height, and the video would be laid out into nothing.
    // Nothing scrolls -- the content is exactly the view.
    //
    // Not covered by a test: the headless stub does not size its content
    // item the way a real Flickable does, so it lays this out correctly
    // either way. This is Silica's own idiom for a page that wants a
    // pull-down and no scrolling.
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
                onClicked: saver.save_as(page.fileUrl, StandardPaths.videos,
                                         page.fileName)
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
            // fillMode left at its default, which is PreserveAspectFit.
            // Naming it would mean naming an enum on the type, which the
            // headless stub cannot carry.
            source: player
            // A video taken upright is stored on its side with the turn
            // written in; this turns it back, as the gallery does.
            autoOrientation: true

            MouseArea {
                anchors.fill: parent
                onClicked: page.toggle()
            }
        }

        // Where the copy went, or why there is none. Above the video,
        // which is laid out below it.
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
                // Never zero: a slider whose ends are the same place has
                // nowhere to put its handle, and duration is 0 until the
                // media has loaded.
                maximumValue: Math.max(1, player.duration)
                onReleased: player.seek(seek.value)
            }

            // Not a binding on `value` itself: dragging the slider writes
            // to that property, which would break a binding and never
            // restore it -- the position would stop following the video
            // after the first seek. `when` puts it back.
            Binding {
                target: seek
                property: "value"
                value: player.position
                when: !seek.down
            }
        }
    }
}
