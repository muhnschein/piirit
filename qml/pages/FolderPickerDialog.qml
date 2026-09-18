import QtQuick 2.0
import Sailfish.Silica 1.0
import Piiri 1.0
import "../components"

/*
 * A folder, chosen by walking to it.
 *
 * Silica has no folder picker a Harbour app may use, so this is one: the
 * folders the sandbox lets the app write in at the top -- Documents,
 * Downloads, Music, Videos, Pictures -- and under each of them whatever
 * is there. A tap goes into a folder; the row above the list goes back
 * up. Swiping forward takes the folder being looked at; swiping back
 * takes nothing. One dialog rather than a page per level, so that a
 * swipe back from anywhere is the reader changing their mind, not one
 * step up.
 *
 * The folder walked into need not exist yet to be the setting -- a copy
 * makes its folder -- but a folder can only be walked into if it does,
 * so the default is offered from the pull-down for a reader who has
 * moved away from it and wants it back.
 */
Dialog {
    id: dialog

    /// The folder to start in. The nearest of its parents that is under
    /// one of the roots is what the dialog opens on; anything else opens
    /// on the roots.
    property string folder: ""

    /// What was chosen. Set on accept; nothing until then.
    property string chosen: ""

    /// The folders the app may write in, by the platform's own names.
    readonly property var roots: [
        StandardPaths.documents, StandardPaths.download, StandardPaths.music,
        StandardPaths.videos, StandardPaths.pictures
    ]

    /// Where the reader is: a path, or "" for the list of roots.
    property string current: ""

    /// Whether `current` is under one of the roots, which is the only
    /// place a folder can be chosen from.
    readonly property bool inFolder: dialog.current.length > 0

    canAccept: dialog.inFolder

    onAccepted: dialog.chosen = dialog.current

    /// The root a path is under, or "" for none.
    function rootOf(path) {
        var text = "" + path
        for (var i = 0; i < dialog.roots.length; i++) {
            var root = "" + dialog.roots[i]
            if (root.length > 0
                    && (text === root || text.indexOf(root + "/") === 0)) {
                return root
            }
        }
        return ""
    }

    /// The name shown for a folder: its own, or the platform's word for
    /// a root.
    function nameOf(path) {
        var text = "" + path
        return text.substring(text.lastIndexOf("/") + 1)
    }

    /// Go into `path`, or to the roots for "".
    function enter(path) {
        dialog.current = "" + path
        folders.path = dialog.current
    }

    /// One level up, to the roots from a root.
    function up() {
        if (!dialog.inFolder) {
            return
        }
        var root = dialog.rootOf(dialog.current)
        if (dialog.current === root) {
            dialog.enter("")
            return
        }
        dialog.enter(dialog.current.substring(0, dialog.current.lastIndexOf("/")))
    }

    /// Start where `folder` is, or as close to it as exists.
    function open() {
        var path = "" + dialog.folder
        var root = dialog.rootOf(path)
        if (root.length === 0) {
            dialog.enter("")
            return
        }
        // Walk up until a folder that is there: the default may not have
        // been made yet, and a folder that was chosen may since be gone.
        while (path !== root) {
            folders.path = path
            if (folders.readable) {
                break
            }
            path = path.substring(0, path.lastIndexOf("/"))
        }
        dialog.enter(path)
    }

    Component.onCompleted: dialog.open()

    FolderList {
        id: folders
        objectName: "folders"
    }

    SilicaListView {
        id: view
        objectName: "folderView"
        anchors.fill: parent

        PullDownMenu {
            MenuItem {
                objectName: "defaultItem"
                //: Puts the folder back to the one a fresh install saves to.
                text: qsTr("Default folder")
                onClicked: {
                    dialog.folder = Settings.defaultSaveFolder
                    dialog.open()
                }
            }
        }

        header: Column {
            width: parent.width

            DialogHeader {
                //: Above the list of folders to choose from.
                title: qsTr("Choose a folder")
            }

            // Where the reader is, and the way back up. At the roots
            // there is no up, and the row says so by not being there.
            BackgroundItem {
                objectName: "upRow"
                width: parent.width
                visible: dialog.inFolder
                height: visible ? Theme.itemSizeSmall : 0
                onClicked: dialog.up()

                Row {
                    x: Theme.horizontalPageMargin
                    height: parent.height
                    spacing: Theme.paddingMedium

                    Image {
                        anchors.verticalCenter: parent.verticalCenter
                        source: "image://theme/icon-m-back"
                    }
                    Label {
                        objectName: "currentLabel"
                        anchors.verticalCenter: parent.verticalCenter
                        width: view.width - 2 * Theme.horizontalPageMargin
                               - Theme.iconSizeMedium - Theme.paddingMedium
                        truncationMode: TruncationMode.Fade
                        color: Theme.highlightColor
                        text: Settings.folderLabel(dialog.current)
                    }
                }
            }
        }

        // The roots by the platform's names, or what is under the folder.
        model: dialog.inFolder ? folders.rows : dialog.roots

        delegate: BackgroundItem {
            objectName: "folderRow"
            width: view.width
            height: Theme.itemSizeSmall
            // A root row's model is the path itself; a folder row's is
            // the model's fields.
            readonly property string rowPath: dialog.inFolder ? model.path : modelData
            onClicked: dialog.enter(rowPath)

            Row {
                x: Theme.horizontalPageMargin
                height: parent.height
                spacing: Theme.paddingMedium

                Image {
                    anchors.verticalCenter: parent.verticalCenter
                    source: "image://theme/icon-m-file-folder"
                }
                Label {
                    objectName: "folderName"
                    anchors.verticalCenter: parent.verticalCenter
                    text: dialog.nameOf(rowPath)
                    color: highlighted ? Theme.highlightColor : Theme.primaryColor
                }
            }
        }

        ViewPlaceholder {
            enabled: dialog.inFolder && folders.count === 0
            //: Under a folder with no folders in it: it can still be chosen.
            text: qsTr("No folders here")
        }

        VerticalScrollDecorator {}
    }
}
