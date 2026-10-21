import QtQuick 2.0
import Sailfish.Share 1.0

/*
 * Viewer's Share: platform's share sheet for one file. Loaded from GalleryPage, not declared:
 * `Sailfish.Share` resolves only on releases shipping it; failed import costs this file, not
 * page (see ShareTarget.qml).
 */
ShareAction {
    id: root
    objectName: "shareAction"

    /// Sheet for file at `fileUrl` (encoded, as views load it).
    function shareFile(fileUrl, mimeType) {
        root.mimeType = mimeType
        root.resources = [String(fileUrl)]
        root.trigger()
    }
}
