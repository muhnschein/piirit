// 2.5 rather than 2.0 for Image.autoTransform; see AttachmentPreview.qml.
import QtQuick 2.5
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0
import "../components"
import "../components"
import "../js/Media.js" as Media
import "../js/Format.js" as Format
import Piirit 1.0

/*
 * Everything of one kind that a chat holds: its pictures and videos as a
 * grid, its sounds, files or apps as a list. Reached from the tiles on
 * the contact's or the group's page (components/MediaKinds.qml), and
 * the same four pages deltachat-android and deltachat-ios keep behind a
 * profile.
 *
 * The grid is the gallery app's: square cells the platform's thumbnailer
 * fills, a play mark over a video, as many columns as fit. The list is
 * the conversation's own attachment rows without the bubbles -- a voice
 * message plays where it sits, an app runs on a tap, a file is handed
 * to whatever opens it, the three answers a tap gets in the chat --
 * with who sent it and when above each.
 *
 * A long press offers, on every kind, the message's place in the chat
 * and its deletion, and on a file what the chat's row menu offers one:
 * a copy, and another app. Deleting waits out the platform's countdown
 * first, held beside the views rather than on the row (PendingRemoval,
 * the conversation's arrangement); showing a message in the chat is a
 * walk back down the stack to the conversation this was opened over.
 *
 * Nothing here is sorted. The core answers newest last, the model turns
 * that round, and the rows stand in that order (chat_media.rs).
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// gallery, audio, files or apps; see ChatMedia.kind.
    property string kind: "gallery"
    property string errorMessage: ""

    readonly property bool isGallery: page.kind === "gallery"

    /// How long a message waits before it goes, in milliseconds. Nothing
    /// sets it; a test turns it down rather than waiting.
    property alias pendingDelay: doomedMessages.delay

    // A grid of pictures is worth turning the phone for.
    allowedOrientations: Orientation.All

    ChatMedia {
        id: media
        objectName: "media"
        account_id: page.accountId
        chat_id: page.chatId
        kind: page.kind
        onError: page.errorMessage = message
    }

    Connections {
        target: core
        // A picture arriving, a message going: the model reads the
        // chat's list again and moves nothing that is still there.
        onCore_event: media.handle_event(context_id, kind, payload_json)
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                media.reload()
            }
        }
    }

    /// The messages the reader has asked to delete, waiting out the
    /// moment in which they can say they did not mean it.
    ///
    /// Beside the views rather than on a row, for the reason the
    /// conversation keeps its own beside the list: deleting is what
    /// destroys rows, and every event reloads them. See PendingRemoval.
    PendingRemoval {
        id: doomedMessages
        // `tag` is which kind of delete was picked on the page the menu
        // led to: true asks the other ends to delete their copies too.
        onRemove: {
            if (tag === true) {
                media.delete_message_for_all(id)
            } else {
                media.delete_message(id)
            }
        }
    }

    // Leaving inside the wait is still asking for the message to go:
    // somebody who asked for a thing to go and then left asked for it to
    // go. The conversation writes its draft at the same moment.
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            doomedMessages.flush()
        }
    }

    /// Ask which kind of delete this is, in the dialog the conversation
    /// asks in: a picture here is a message in that chat, and the two
    /// ways to delete one are the same either way. The wait starts when
    /// the dialog is accepted, and nothing goes until it is up.
    ///
    /// The gate is read now rather than in the answer: the row may be
    /// gone by the time the reader has chosen.
    function askDelete(messageId, canDeleteForEveryone) {
        var chooser = pageStack.push(
            Qt.resolvedUrl("DeleteMessageDialog.qml"),
            { canDeleteForEveryone: canDeleteForEveryone })
        if (chooser) {
            chooser.picked.connect(function(forEveryone) {
                doomedMessages.ask(messageId, forEveryone)
            })
        }
    }

    /// Silica's own countdown, drawn over what is going: the bar, the
    /// seconds, "Tap to cancel", all of it the platform's. Built the
    /// first time a row asks, in the row's own `loader`, since every row
    /// would otherwise carry a dozen items for a tap that almost never
    /// comes. The deletion is not its business -- that belongs to
    /// `doomedMessages`, which outlives the row -- so it is handed a
    /// callback that does nothing and asked only to draw and to report
    /// the tap.
    function raiseRemorse(cell, loader, over, messageId) {
        // Once per wait: the answer that starts one and a rebuild of the
        // row inside it both come through here, and two `execute` calls
        // over one item is a countdown restarting under the reader.
        if (cell.countingDown) {
            return
        }
        cell.countingDown = true
        loader.active = true
        //: What Silica's countdown says it is doing, over a picture, a
        //: sound, a file or an app the reader has asked to delete.
        loader.item.execute(over, qsTr("Deleting"), function() {},
                            doomedMessages.countdownFor(messageId))
    }

    /// A path the other end named, as a URL the views can load; empty
    /// for a row that has not been read yet.
    function fileUrlOf(path) {
        return path.length > 0 ? Qt.resolvedUrl(Media.fileUrl(path)) : ""
    }

    /// What a tap opens: the conversation's own answer for each kind
    /// (ConversationPage.openAttachment), so a picture found here opens
    /// on the page it opens on in the chat, and a file goes to whatever
    /// else on the phone handles it.
    function open(fileUrl, fileName, viewType) {
        // A row not read yet has no file to open.
        if (String(fileUrl).length === 0) {
            return
        }
        if (viewType === "Image" || viewType === "Gif"
                || viewType === "Sticker") {
            pageStack.push(Qt.resolvedUrl("PicturePage.qml"), {
                fileUrl: fileUrl,
                fileName: fileName,
                viewType: viewType
            })
        } else if (viewType === "Video") {
            pageStack.push(Qt.resolvedUrl("VideoPage.qml"), {
                fileUrl: fileUrl,
                fileName: fileName
            })
        } else {
            Qt.openUrlExternally(fileUrl)
        }
    }

    /// A webxdc app, run here; see ConversationPage.openApp.
    function openApp(messageId) {
        pageStack.push(Qt.resolvedUrl("WebxdcPage.qml"), {
            accountId: page.accountId,
            messageId: messageId
        })
    }

    /// Back to the conversation, at this message: where a search result
    /// lands, asked for from here. The conversation is under the
    /// contact's or the group's page that opened this one, so the walk
    /// down the stack stops at the first page that can be asked. Told
    /// first and popped to second: the conversation keeps the ask until
    /// it is the page on screen and acts on it then, over the place it
    /// puts back on its way in (ConversationPage.showMessage).
    function showInChat(messageId) {
        var below = pageStack.previousPage(page)
        while (below && typeof below.showMessage !== "function") {
            below = pageStack.previousPage(below)
        }
        if (!below) {
            return
        }
        below.showMessage(messageId)
        pageStack.pop(below)
    }

    // A copy into Downloads, under the name the sender gave it: what the
    // conversation's row menu does with a file, and the same words for
    // it.
    FileSaver {
        id: saver
        objectName: "saver"
        onSaved: notice.show(qsTr("Saved to Downloads"))
        onError: page.errorMessage = message
    }

    // Two views for four kinds, one of them built: a grid draws
    // pictures and a list draws everything else, and a view that is not
    // the one showing would still build a delegate per row.
    Loader {
        id: view
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
            bottom: errorBanner.top
        }
        sourceComponent: page.isGallery ? gallery : listing
    }

    // Only until there is something to look at: a list that is filling
    // in from the top says so by filling in.
    BusyIndicator {
        objectName: "mediaBusy"
        anchors.centerIn: view
        size: BusyIndicatorSize.Large
        running: media.loading && media.count === 0
    }

    Component {
        id: gallery

        SilicaGridView {
            id: grid
            objectName: "mediaGrid"
            anchors.fill: parent
            clip: true
            model: media.rows

            /// As many columns as the platform's largest item fits
            /// across the width: three on a phone held upright, more on
            /// its side.
            readonly property int columns:
                Math.max(1, Math.floor(width / Theme.itemSizeExtraLarge))
            cellWidth: Math.floor(width / columns)
            cellHeight: cellWidth

            header: PageHeader {
                title: Media.kindName(page.kind)
            }

            // Silica's grid cell with a menu on it, as its ListItem is
            // the row with one: a long press opens the menu under the
            // row of cells, the way the notes app's grid does.
            delegate: GridItem {
                id: tile
                objectName: "mediaTile" + model.message_id
                contentWidth: grid.cellWidth
                contentHeight: grid.cellHeight

                readonly property bool isVideo: model.view_type === "Video"
                readonly property bool isAnimated: model.view_type === "Gif"
                /// The file, once the row has been read.
                readonly property url fileUrl:
                    model.loaded ? page.fileUrlOf(model.file_path) : ""
                /// Half the gap between two tiles.
                readonly property real inset: Theme.paddingSmall / 2
                /// This picture is on its way out.
                readonly property bool doomed:
                    doomedMessages.pending(model.message_id)
                /// Whether this tile has already put a countdown up over
                /// the wait it is in; see `page.raiseRemorse`.
                property bool countingDown: false

                Loader {
                    id: remorse
                    active: false
                    sourceComponent: RemorseItem {
                        objectName: "mediaRemorse"
                        onCanceled: doomedMessages.spare(model.message_id)
                    }
                }

                // The wait is asked for on a page now, so it starts while
                // the menu is long gone: the tile hears of it here.
                onDoomedChanged: {
                    if (tile.doomed) {
                        page.raiseRemorse(tile, remorse, body, model.message_id)
                    } else {
                        tile.countingDown = false
                    }
                }

                // A tile is rebuilt every time it scrolls back into view,
                // so one scrolled past mid-wait comes back with no
                // countdown on it. Put it up again with what is left.
                Component.onCompleted: {
                    if (tile.doomed) {
                        page.raiseRemorse(tile, remorse, body, model.message_id)
                    }
                }

                menu: ContextMenu {
                    MenuItem {
                        objectName: "showItem"
                        //: Goes back to the conversation, at this message.
                        text: qsTr("Show in chat")
                        onClicked: page.showInChat(model.message_id)
                    }
                    MenuItem {
                        objectName: "deleteItem"
                        text: qsTr("Delete")
                        // The page is told, not this tile: the wait
                        // before a message goes has to outlive the tile
                        // it was asked for on; which kind of delete
                        // this is belongs to a dialog of its own. See
                        // PendingRemoval and DeleteMessageDialog.
                        onClicked: page.askDelete(
                                       model.message_id,
                                       model.is_outgoing && model.show_padlock
                                       && !model.is_info)
                    }
                }

                // Everything drawn in the cell, so the countdown can cover
                // it: faded by the countdown while the picture waits to
                // go, and out of a tap's reach meanwhile.
                Item {
                    id: body
                    width: tile.contentWidth
                    height: tile.contentHeight
                    enabled: !tile.doomed

                    // What stands in the cell until the thumbnailer has
                    // drawn into it: a row not read yet, or a file it has
                    // not got to.
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: tile.inset
                        color: Theme.rgba(Theme.highlightDimmerColor, 0.4)
                    }

                    // The platform's thumbnailer, for pictures and videos
                    // alike: it draws to the cell's own size and keeps
                    // what it drew, which is what the gallery app scrolls
                    // through -- not a decode of the whole picture per
                    // tile.
                    Thumbnail {
                        objectName: "tileThumbnail"
                        anchors.fill: parent
                        anchors.margins: tile.inset
                        sourceSize.width: width
                        sourceSize.height: height
                        // Told what the file is, since the thumbnailer
                        // picks its reader by that; a message the core
                        // gave no type for is read as the kind of thing
                        // the tile says it is.
                        mimeType: model.file_mime.length > 0 ? model.file_mime
                                  : tile.isVideo ? "video/mp4" : "image/jpeg"
                        source: tile.fileUrl
                    }

                    // The marks the conversation draws on the same two
                    // kinds.
                    Rectangle {
                        objectName: "playMark"
                        visible: tile.isVideo || tile.isAnimated
                        anchors.centerIn: parent
                        width: Theme.itemSizeExtraSmall
                        height: width
                        radius: width / 2
                        color: Theme.rgba("black", 0.5)

                        Label {
                            anchors.centerIn: parent
                            color: "white"
                            font.pixelSize: tile.isVideo ? Theme.fontSizeMedium
                                                         : Theme.fontSizeExtraSmall
                            font.bold: tile.isAnimated
                            textFormat: Text.PlainText
                            // The play mark and the format's name, neither
                            // of them a word to translate.
                            text: tile.isVideo ? "▶" : "GIF"
                        }
                    }
                }

                onClicked: page.open(tile.fileUrl, model.file_name, model.view_type)
            }

            ViewPlaceholder {
                objectName: "placeholder"
                enabled: media.loaded && media.count === 0
                text: Media.emptyText(page.kind)
            }

            VerticalScrollDecorator {}
        }
    }

    Component {
        id: listing

        SilicaListView {
            id: list
            objectName: "mediaList"
            anchors.fill: parent
            clip: true
            model: media.rows

            header: PageHeader {
                title: Media.kindName(page.kind)
            }

            delegate: ListItem {
                id: row
                objectName: "mediaRow" + model.message_id
                width: list.width
                contentHeight: content.height

                readonly property bool isFile: page.kind === "files"
                readonly property bool isApp: page.kind === "apps"
                /// The file, once the row has been read.
                readonly property url fileUrl:
                    model.loaded ? page.fileUrlOf(model.file_path) : ""
                /// This message is on its way out.
                readonly property bool doomed:
                    doomedMessages.pending(model.message_id)
                /// Whether this row has already put a countdown up; see
                /// the tile.
                property bool countingDown: false

                // The wait starts on the page the menu leads to, by which
                // time the menu is gone; see the tile.
                onDoomedChanged: {
                    if (row.doomed) {
                        page.raiseRemorse(row, remorse, content, model.message_id)
                    } else {
                        row.countingDown = false
                    }
                }

                Loader {
                    id: remorse
                    active: false
                    sourceComponent: RemorseItem {
                        objectName: "mediaRemorse"
                        onCanceled: doomedMessages.spare(model.message_id)
                    }
                }

                // A row rebuilt mid-wait comes back bare; see the tile.
                Component.onCompleted: {
                    if (row.doomed) {
                        page.raiseRemorse(row, remorse, content, model.message_id)
                    }
                }

                menu: ContextMenu {
                    MenuItem {
                        objectName: "showItem"
                        //: Goes back to the conversation, at this message.
                        text: qsTr("Show in chat")
                        onClicked: page.showInChat(model.message_id)
                    }
                    MenuItem {
                        objectName: "openItem"
                        //: Hands the attachment to whatever else on the phone
                        //: handles files of its kind.
                        text: qsTr("Open in another app")
                        onClicked: Qt.openUrlExternally(row.fileUrl)
                    }
                    MenuItem {
                        objectName: "saveItem"
                        text: qsTr("Save to device")
                        onClicked: saver.save_as(row.fileUrl, StandardPaths.download,
                                                 model.file_name)
                    }
                    MenuItem {
                        objectName: "deleteItem"
                        text: qsTr("Delete")
                        // The page is told, not this row; see the tile.
                        onClicked: page.askDelete(
                                       model.message_id,
                                       model.is_outgoing && model.show_padlock
                                       && !model.is_info)
                    }
                }

                // The caption and the row under it, together, so the
                // countdown can cover both: faded by it while the message
                // waits to go, and out of a tap's reach meanwhile.
                Item {
                    id: content
                    width: parent.width
                    height: body.y + body.height + Theme.paddingMedium
                    enabled: !row.doomed

                    // Who sent it and when. The name is theirs to choose.
                    Label {
                        id: caption
                        objectName: "rowCaption"
                        x: Theme.horizontalPageMargin
                        y: Theme.paddingMedium
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        truncationMode: TruncationMode.Fade
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: Theme.secondaryHighlightColor
                        textFormat: Text.PlainText
                        text: model.loaded
                              ? model.sender_name + " · "
                                + Qt.formatDate(new Date(model.timestamp * 1000),
                                                Qt.DefaultLocaleShortDate)
                              : ""
                    }

                    // The two shapes a row takes, each zero-high when it
                    // is not the one showing, so the row's height is their
                    // sum.
                    Item {
                        id: body
                        x: Theme.horizontalPageMargin
                        y: caption.y + caption.height + Theme.paddingSmall
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        height: fileRow.height + preview.height

                        // A file: the theme's icon for its kind, its name
                        // and its size -- the platform's own file
                        // browser's row.
                        Item {
                            id: fileRow
                            readonly property bool shown: row.isFile
                            visible: fileRow.shown
                            width: parent.width
                            height: fileRow.shown
                                    ? Math.max(fileIcon.height,
                                               fileName.height + fileDetail.height)
                                    : 0

                            Image {
                                id: fileIcon
                                objectName: "fileIcon"
                                anchors.verticalCenter: parent.verticalCenter
                                width: Theme.iconSizeMedium
                                height: width
                                source: fileRow.shown && model.loaded
                                        ? "image://theme/" + Media.fileIcon(model.file_mime)
                                          + "?" + (row.highlighted ? Theme.highlightColor
                                                                    : Theme.primaryColor)
                                        : ""
                            }

                            Label {
                                id: fileName
                                objectName: "fileName"
                                x: fileIcon.width + Theme.paddingMedium
                                width: parent.width - x
                                truncationMode: TruncationMode.Fade
                                color: row.highlighted ? Theme.highlightColor
                                                       : Theme.primaryColor
                                textFormat: Text.PlainText
                                text: model.file_name.length > 0 ? model.file_name
                                                                 : model.file_path
                            }

                            Label {
                                id: fileDetail
                                objectName: "fileDetail"
                                x: fileName.x
                                y: fileName.height
                                width: fileName.width
                                truncationMode: TruncationMode.Fade
                                font.pixelSize: Theme.fontSizeExtraSmall
                                color: Theme.secondaryColor
                                textFormat: Text.PlainText
                                text: Format.readableSize(model.file_bytes)
                            }
                        }

                        // A sound or an app: the conversation's own
                        // rendering of it, player and all. Given no file
                        // for a file, which the row above draws, so it
                        // measures nothing.
                        AttachmentPreview {
                            id: preview
                            objectName: "attachment"
                            y: fileRow.height
                            contentWidth: parent.width
                            filePath: row.isFile || !model.loaded ? "" : model.file_path
                            fileName: model.file_name
                            fileMime: model.file_mime
                            fileBytes: model.file_bytes
                            viewType: row.isFile ? "Text" : model.view_type
                            webxdcName: model.webxdc_name
                            webxdcDocument: model.webxdc_document
                            webxdcSummary: model.webxdc_summary
                            webxdcIcon: model.webxdc_icon
                            // The apps page is offered only where apps are
                            // on, so a row here draws as the app it is.
                            appsEnabled: true
                            onMenuRequested: row.openMenu()
                        }
                    }
                }

                // A file is opened and an app is run, as in the chat; a
                // sound has its own play button.
                onClicked: {
                    if (row.isApp) {
                        page.openApp(model.message_id)
                    } else if (row.isFile) {
                        page.open(row.fileUrl, model.file_name, model.view_type)
                    }
                }
            }

            ViewPlaceholder {
                objectName: "placeholder"
                enabled: media.loaded && media.count === 0
                text: Media.emptyText(page.kind)
            }

            VerticalScrollDecorator {}
        }
    }

    Banner {
        id: errorBanner
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }

    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 4
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        onDismissed: notice.text = ""
    }
}
