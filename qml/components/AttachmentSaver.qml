import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0

/*
 * A copy of a file from a chat, where the reader can find it again:
 * Piirit's own folder in Downloads, whatever kind of file it is. One
 * folder for every kind, so there is one place to look, and one that is
 * Piirit's, so what was kept from a chat is not mixed in with everything
 * else on the phone. The sandbox grants Downloads and what is under it
 * (UserDirs); the folder is made on the first save.
 *
 * Every page that keeps a copy saves through this -- a row's menu in the
 * conversation, a picture, a video, a chat's media, a file a webxdc app
 * hands over -- and says `savedText` once it has.
 */
FileSaver {
    id: saver

    /// Where every copy goes.
    readonly property string folder: StandardPaths.download + "/Piirit"

    //: Said once a file from a chat has been copied: the folder named
    //: Piirit inside Downloads. Keep "Piirit", which is the folder's name.
    readonly property string savedText: qsTr("Saved to Downloads/Piirit")

    /// Copy the file at `fileUrl` into the folder under `fileName`, the
    /// name the sender gave it -- or under the file's own, when that is
    /// empty. Answers on `saved` or `error`.
    function keep(fileUrl, fileName) {
        saver.save_as(fileUrl, saver.folder, fileName)
    }
}
