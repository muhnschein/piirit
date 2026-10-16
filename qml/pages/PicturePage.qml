import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * One picture alone (profile picture, sticker); here, not system handler (leaves Piirit,
 * fails on some devices). Chat pictures and videos swipe: GalleryPage. See PictureView.
 */
Page {
    id: page

    /// File, already encoded: see AttachmentPreview.fileUrl.
    property url fileUrl
    property string fileName
    /// Image, Gif or Sticker: core enum.
    property string viewType: "Image"
    /// Width conversation drew it at; 0 if not from there. See PictureView.
    property real previewWidth: 0

    /// Page in place (transition over, or none). Until then picture waits; see PictureView.
    property bool settled: false
    onStatusChanged: if (page.status === PageStatus.Active) page.settled = true
    Component.onCompleted: if (page.status === PageStatus.Active) page.settled = true

    PictureView {
        objectName: "pictureView"
        anchors.fill: parent
        fileUrl: page.fileUrl
        fileName: page.fileName
        viewType: page.viewType
        previewWidth: page.previewWidth
        settled: page.settled
    }
}
