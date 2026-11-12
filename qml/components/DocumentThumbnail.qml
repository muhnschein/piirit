// 2.5 as AttachmentPreview: one QtQuick version per page.
import QtQuick 2.5
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0
import "../js/Media.js" as Media

/*
 * File's first page, small: PDF by platform thumbnailer (cached, own process), office file
 * by picture its writer stored inside (src/page_preview.rs). Neither there: type's icon.
 */
Item {
    id: root

    /// Encoded `file://` URL; empty: nothing read.
    property string fileUrl: ""
    property string fileMime: ""
    property bool highlighted: false
    /// Icon's side; whole item when no page drawn.
    property real iconSize: Theme.iconSizeMedium

    /// "pdf", "page" or "" (Media.previewKind).
    readonly property string kind: root.fileUrl.length > 0 ? Media.previewKind(root.fileMime) : ""
    /// Page picture drawn (PDF: asked for; thumbnailer tells nothing back).
    readonly property bool drawn: stored.status === Image.Ready

    clip: true

    Image {
        objectName: "documentIcon"
        anchors.centerIn: parent
        width: root.iconSize
        height: width
        visible: !root.drawn
        source: "image://theme/" + Media.fileIcon(root.fileMime)
                + "?" + (root.highlighted ? Theme.highlightColor : Theme.primaryColor)
    }

    Loader {
        active: root.kind === "pdf"
        sourceComponent: Component {
            // Draws over icon once made; failed: stays clear, icon shows. Default fill crops
            // to page's middle, title lost: whole page asked for, as wide as tile; its top
            // shown, or all of it centred when shorter than tile. A4 until thumbnail tells.
            Thumbnail {
                objectName: "documentPdf"
                width: root.width
                height: implicitWidth > 0 ? Math.round(width * implicitHeight / implicitWidth)
                                          : Math.round(width * 1.414)
                y: Math.max(0, Math.round((root.height - height) / 2))
                fillMode: Thumbnail.PreserveAspectFit
                sourceSize.width: width
                sourceSize.height: 2 * width
                mimeType: "application/pdf"
                source: root.fileUrl
            }
        }
    }

    Image {
        id: stored
        objectName: "documentPage"
        anchors.fill: parent
        visible: root.drawn
        asynchronous: true
        sourceSize.width: Math.round(width)
        sourceSize.height: Math.round(height)
        source: root.kind === "page" && width > 0
                ? "image://piirit/page?file=" + encodeURIComponent(root.fileUrl)
                : ""
    }
}
