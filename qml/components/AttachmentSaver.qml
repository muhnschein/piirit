import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0

/*
 * Chat file copies into Piirit's Downloads folder (sandbox grants Downloads;
 * made on first save). Pages then show `savedText`.
 */
FileSaver {
    id: saver

    /// Destination for all copies.
    readonly property string folder: StandardPaths.download + "/Piirit"

    //: Said once a file from a chat has been copied: the folder named
    //: Piirit inside Downloads. Keep "Piirit", which is the folder's name.
    readonly property string savedText: qsTr("Saved to Downloads/Piirit")

    /// Copy `fileUrl` as `fileName` (or own name). Off Qt thread (slow for video), `busy`
    /// meanwhile; answers `saved` or `error`, in request order.
    function keep(fileUrl, fileName) {
        saver.save_as(fileUrl, saver.folder, fileName)
    }
}
