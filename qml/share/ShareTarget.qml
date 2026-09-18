import QtQuick 2.0
import Sailfish.Share 1.0

/*
 * What the platform hands this app when something is shared to it: a
 * picture from the gallery, a link from the browser, a file from the file
 * manager.
 *
 * An app becomes one of the entries in the share sheet by saying so in
 * its own desktop file -- `X-Share-Methods`, and a group per method
 * naming what it is called and which types it takes -- and by having a
 * `ShareProvider` of the same name running in it. Nothing is installed
 * outside the app for this: the older way, a plugin in the transfer
 * engine's own directory, is not open to a Harbour package and this is.
 *
 * Loaded rather than declared, from piiri.qml, for the reason the
 * webxdc pages are pushed by URL: `Sailfish.Share` resolves only on a
 * release that has it, and an import that does not resolve must cost this
 * file rather than the window every page is loaded into.
 *
 * What arrives is taken to the window, which asks which chat it is for.
 */
Item {
    id: root

    /*
     * The words the share sheet shows.
     *
     * It reads them from the desktop file, which no catalogue reaches --
     * a desktop entry carries its own translations, as `Description[de]`
     * and so on. They are written here as well so `lupdate` collects
     * them and the desktop file can be filled from the same catalogues
     * everything else is translated in. harbour-maelstrom, which is
     * where this app learnt the mechanism, does the same.
     *
     * Both must match the `Description=` of the group of the same name
     * in harbour-piiri.desktop, which tests/qml_syntax.rs checks.
     */
    //: Shown in the phone's share sheet, for a file shared to this app.
    readonly property string filesDescription: qsTr("Send in a chat")
    //: Shown in the phone's share sheet, for text or a link shared to
    //: this app.
    readonly property string textDescription: qsTr("Send as a message")

    /// A file: the gallery's picture, the file manager's document.
    ShareProvider {
        objectName: "shareFiles"
        method: "files"
        onTriggered: root.take(resources)
    }

    /// Text, and the links that are text with a title on them.
    ShareProvider {
        objectName: "shareText"
        method: "text"
        capabilities: ["text/*", "text/x-url", "text/uri-list"]
        onTriggered: root.take(resources)
    }

    /// Take what was shared to the window, which knows what to do with
    /// it.
    ///
    /// The first file, or the first piece of text: the desktop file says
    /// neither method takes more than one, and the conversation carries
    /// one attachment at a time.
    ///
    /// Told apart by what each resource carries rather than by
    /// `ShareResource.FilePathType` and its sibling. Those names cannot
    /// be stood in for -- QML refuses a property whose name begins with
    /// a capital, so no stub can offer them -- and a comparison that
    /// only resolves on a phone is one nothing here could check. A
    /// resource has a path or it has data, and that is the same
    /// question.
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
        // The window first: a share can arrive while the app is behind
        // something else, and a chat picker nobody can see is worse than
        // no picker at all.
        appWindow.activate()
        appWindow.shareInto(filePath, text)
    }
}
