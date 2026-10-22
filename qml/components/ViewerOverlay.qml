import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Over viewer's slides, as Jolla Gallery's (GalleryOverlay, sailfish-components-gallery,
 * BSD-3-Clause, Jolla Ltd.): fades in and out on a tap. Top: who sent it and when, left (Jolla's
 * has a details button there); close right. Bottom: video's seek bar, icon bar under it: Share,
 * Save to device.
 *
 * Rebuilt, not imported: Sailfish.Gallery isn't a Harbour import.
 */
Item {
    id: root

    /// Up; fades. Page toggles on tap.
    property bool shown: true
    /// Slide's video (VideoView), or null: seek bar.
    property Item video: null
    /// File there: Share, Save. Not yet downloaded one has none.
    property bool hasFile: true
    /// Share sheet there (Sailfish.Share: newer releases only).
    property bool canShare: true
    /// Top line: sender, day. Empty: none.
    property string caption
    /// Copy running: Save off (second press, second copy).
    property bool saving: false

    readonly property bool isVideo: root.video !== null
    /// Seek bar held: page keeps overlay up meanwhile.
    readonly property bool seeking: seek.down

    signal closeRequested()
    signal shareRequested()
    signal saveRequested()

    enabled: root.shown
    opacity: root.shown ? 1 : 0
    visible: opacity > 0
    Behavior on opacity { NumberAnimation { duration: 200 } }

    // Dark behind buttons: readable over a light photo.
    Rectangle {
        objectName: "topFade"
        width: parent.width
        height: dismiss.height + 2 * dismiss.y
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.6) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    Label {
        objectName: "caption"
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: dismiss.left
            rightMargin: Theme.paddingMedium
            verticalCenter: dismiss.verticalCenter
        }
        text: root.caption
        font.pixelSize: Theme.fontSizeSmall
        truncationMode: TruncationMode.Fade
    }

    IconButton {
        id: dismiss
        objectName: "closeButton"
        anchors.right: parent.right
        anchors.rightMargin: Theme.horizontalPageMargin
        y: Theme.paddingLarge
        icon.source: "image://theme/icon-m-dismiss"
        onClicked: root.closeRequested()
    }

    Rectangle {
        objectName: "bottomFade"
        anchors.bottom: parent.bottom
        width: parent.width
        height: bar.height + 2 * bar.anchors.bottomMargin
        gradient: Gradient {
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.6) }
        }
    }

    Column {
        id: bar
        width: parent.width
        anchors.bottom: parent.bottom
        anchors.bottomMargin: parent.height / 20

        Slider {
            id: seek
            objectName: "seek"
            width: parent.width
            visible: root.isVideo
            handleVisible: false
            minimumValue: 0
            // Never zero: equal ends leave no handle room; length 0 until known.
            maximumValue: Math.max(1, root.video ? root.video.length : 0)
            valueText: root.video ? root.video.clock(seek.value) : ""
            onReleased: if (root.video) root.video.seekTo(seek.value)
        }

        // Not bound to `value`: drag breaks binding for good. `when` restores.
        Binding {
            target: seek
            property: "value"
            value: root.video ? root.video.position : 0
            when: !seek.down
        }

        Row {
            objectName: "iconBar"
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Theme.paddingLarge
            visible: root.hasFile

            IconButton {
                objectName: "shareButton"
                visible: root.canShare
                icon.source: "image://theme/icon-m-share"
                onClicked: root.shareRequested()
            }
            IconButton {
                objectName: "saveButton"
                enabled: !root.saving
                icon.source: "image://theme/icon-m-device-download"
                onClicked: root.saveRequested()
            }
        }
    }
}
