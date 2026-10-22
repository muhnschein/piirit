import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Picture or video not yet downloaded, as gallery slide: what its conversation row says (core's
 * text, "[Image – 2.4 MiB]") and Download. Core keeps such as name and size until fetched
 * (pre-message); once fetched, same message has picture, slide turns PictureView.
 *
 * Light colours whatever the ambience: gallery is black.
 */
Item {
    id: root

    /// Kind its file will be: Image, Gif, Video (chat_media.rs `held_kind`).
    property string viewType: "Image"
    /// Core's text: caption, then kind and size, as conversation row.
    property string text: ""
    /// Core's downloadState: Available, InProgress, Failure, Undecipherable.
    property string downloadState: "Available"

    /// States offering fetch, as conversation's.
    readonly property bool canDownload: root.downloadState === "Available"
                                        || root.downloadState === "Failure"

    /// Fetch asked; page tells model.
    signal downloadRequested()

    Column {
        anchors.verticalCenter: parent.verticalCenter
        x: Theme.horizontalPageMargin
        width: parent.width - 2 * Theme.horizontalPageMargin
        spacing: Theme.paddingLarge

        // Kind's theme icon: nothing to draw yet.
        Image {
            objectName: "heldIcon"
            anchors.horizontalCenter: parent.horizontalCenter
            width: Theme.iconSizeLarge
            height: width
            source: "image://theme/"
                    + (root.viewType === "Video" ? "icon-m-file-video" : "icon-m-file-image")
                    + "?" + Theme.lightSecondaryColor
        }

        Label {
            objectName: "heldText"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            color: Theme.lightPrimaryColor
            // Sender's caption: plain, as conversation's.
            textFormat: Text.PlainText
            // Core leads with a space when no caption.
            text: root.text.trim()
        }

        // Fetching, failed, unreadable: what came of Download.
        Label {
            objectName: "heldState"
            visible: text.length > 0
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            color: Theme.lightSecondaryColor
            font.pixelSize: Theme.fontSizeSmall
            textFormat: Text.PlainText
            text: root.downloadState === "InProgress"
                  ? qsTr("Downloading…")
                  : root.downloadState === "Failure"
                    //: Under a picture or video not yet downloaded, after trying failed;
                    //: Download button below tries again.
                    ? qsTr("Download failed")
                    : root.downloadState === "Undecipherable"
                      ? qsTr("Cannot be decrypted")
                      : ""
        }

        BusyIndicator {
            objectName: "heldBusy"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: running
            running: root.downloadState === "InProgress"
            size: BusyIndicatorSize.Medium
        }

        Button {
            objectName: "downloadButton"
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.canDownload
            //: Fetches a picture or video not yet downloaded, in the full-screen viewer.
            text: qsTr("Download")
            onClicked: root.downloadRequested()
        }
    }
}
