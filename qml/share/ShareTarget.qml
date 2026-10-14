import QtQuick 2.0
import Sailfish.Share 1.0

/*
 * What the platform hands the app when something is shared: picture, link, or file.
 * Share-sheet entry: `X-Share-Methods` plus a group per method in the desktop file, and a
 * `ShareProvider` of the same name. Nothing installed outside the app (transfer-engine
 * plugin dir is closed to Harbour). Loaded, not declared (from piirit.qml): a failed
 * `Sailfish.Share` import must cost this file, not the window. Shares go to the window.
 */
Item {
    id: root

    /*
     * Share sheet words. Sheet reads the desktop file (own translations, no catalogue);
     * here so `lupdate` collects them and desktop file is filled from catalogues (as
     * harbour-maelstrom). Must match `Description=` of same-named group in
     * harbour-piirit.desktop (checked by tests/qml_syntax.rs).
     */
    //: Shown in the phone's share sheet, for a file shared to this app.
    readonly property string filesDescription: qsTr("Send in a chat")
    //: Shown in the phone's share sheet, for text or a link shared to
    //: this app.
    readonly property string textDescription: qsTr("Send as a message")

    /// File: gallery picture or document.
    ShareProvider {
        objectName: "shareFiles"
        method: "files"
        onTriggered: root.take(resources)
    }

    /// Text, and links with a title.
    ShareProvider {
        objectName: "shareText"
        method: "text"
        capabilities: ["text/*", "text/x-url", "text/uri-list"]
        onTriggered: root.take(resources)
    }

    /// Shared item to the window, which knows what to do with it.
    ///
    /// First file or text: desktop file allows one each.
    ///
    /// Told apart by resource contents, not `ShareResource.FilePathType`.
    /// QML refuses capitalised property names, so stubs can't offer that.
    function take(resources) {
        var filePath = ""
        var text = ""
        for (var i = 0; resources && i < resources.length; i++) {
            var one = resources[i]
            if (filePath.length === 0 && one.filePath
                    && ("" + one.filePath).length > 0) {
                filePath = "" + one.filePath
            } else if (text.length === 0 && one.data
                       && ("" + one.data).length > 0) {
                text = "" + one.data
            }
        }
        if (filePath.length === 0 && text.length === 0) {
            return
        }
        // Window first: picker hidden behind the app is worse than none.
        appWindow.activate()
        appWindow.shareInto(filePath, text)
    }
}
