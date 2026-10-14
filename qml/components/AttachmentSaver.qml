import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0

/*
 * Chat file copies in Piirit's Downloads folder: findable, apart from other
 * phone files. Sandbox grants Downloads; made on first save. Pages save
 * here, then show `savedText`.
 */
FileSaver {
    id: saver

    /// Destination for all copies.
    readonly property string folder: StandardPaths.download + "/Piirit"

    //: Said once a file from a chat has been copied: the folder named
    //: Piirit inside Downloads. Keep "Piirit", which is the folder's name.
    readonly property string savedText: qsTr("Saved to Downloads/Piirit")

    /// Copy `fileUrl` to folder as `fileName` (or file's own if empty).
    /// Answers `saved` or `error`, off Qt thread (slow for video); `busy`
    /// meanwhile. Concurrent requests answered in order.
    function keep(fileUrl, fileName) {
        saver.save_as(fileUrl, saver.folder, fileName)
    }
}
