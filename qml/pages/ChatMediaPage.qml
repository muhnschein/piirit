// 2.5 not 2.0: Image.autoTransform. See AttachmentPreview.qml.
import QtQuick 2.5
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0
import "../components"
import "../components"
import "../js/Media.js" as Media
import "../js/Format.js" as Format
import Piirit 1.0

/*
 * One kind of chat media: grid (pictures, videos) or list (rest).
 * From contact/group tiles (components/MediaKinds.qml); same four pages as
 * deltachat-android/ios.
 * Grid: gallery-style cells filled by platform thumbnailer, play mark on video.
 * List: conversation's attachment rows minus bubbles (voice plays in place,
 * app runs, file handed to other app), sender and time above.
 * Long press: go to message, or delete; files also copy, open with. Delete
 * waits out countdown beside views (PendingRemoval). Show in chat walks back
 * down stack to conversation.
 * Core sends newest last, model reverses: rows newest first, no sorting here
 * (chat_media.rs).
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Gallery, audio, files or apps (ChatMedia.kind).
    property string kind: "gallery"
    property string errorMessage: ""

    readonly property bool isGallery: page.kind === "gallery"

    /// Ms before message goes. Tests set low.
    property alias pendingDelay: doomedMessages.delay

    // Grid of pictures: worth turning the phone.
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
        // Model re-reads list on arrival or delete; moves nothing still there.
        onCore_event: media.handle_event(context_id, kind, payload_json)
        // Model made before core is up has nothing to load.
        onStatus_changed: {
            if (core.status === "ready") {
                media.reload()
            }
        }
    }

    /// Messages awaiting delete (undo wait).
    ///
    /// Beside views, not on rows: deleting destroys rows, every event reloads
    /// them. See PendingRemoval.
    PendingRemoval {
        id: doomedMessages
        // `tag`: delete kind picked in DeleteMessageDialog. True asks other ends
        // too.
        onRemove: {
            if (tag === true) {
                media.delete_message_for_all(id)
            } else {
                media.delete_message(id)
            }
        }
    }

    // Leaving mid-wait still deletes. Conversation writes its draft then too.
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            doomedMessages.flush()
        }
    }

    /// Ask delete kind in the conversation's dialog. Wait starts on accept.
    ///
    /// Gate read now: row may be gone by answer.
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

    /// Silica's countdown over item. Built on first ask (row `loader`), not per
    /// row. Deletion is `doomedMessages`'s (outlives row): given no-op
    /// callback, only draws, reports tap.
    function raiseRemorse(cell, loader, over, messageId) {
        // Once per wait: start and rebuild both land here. Two `execute` calls
        // restart countdown.
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

    /// Path as URL views load. Empty for empty path (row not loaded yet).
    function fileUrlOf(path) {
        return path.length > 0 ? Qt.resolvedUrl(Media.fileUrl(path)) : ""
    }

    /// Tap opens per kind (ConversationPage.openAttachment). Files go to other
    /// apps.
    function open(fileUrl, fileName, viewType) {
        // Row not loaded yet: no file.
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

    /// Webxdc app, run here. See ConversationPage.openApp.
    function openApp(messageId) {
        pageStack.push(Qt.resolvedUrl("WebxdcPage.qml"), {
            accountId: page.accountId,
            messageId: messageId
        })
    }

    /// Back to conversation at this message. Walks down stack to first page
    /// with `showMessage` (under contact/group page). Told first, popped to
    /// second: it acts once on screen, over restored place
    /// (ConversationPage.showMessage).
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

    // Copy under sender's name, as conversation's row menu does. See
    // AttachmentSaver.
    AttachmentSaver {
        id: saver
        objectName: "saver"
        onSaved: notice.show(saver.savedText)
        onError: page.errorMessage = message
    }

    // Two views: grid for pictures, list for rest. Loader builds only the
    // shown one: hidden view would still build delegates.
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

    // Only while filling: a list filling from top says so itself.
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

            /// Columns: as many platform largest items as fit. Three upright.
            readonly property int columns:
                Math.max(1, Math.floor(width / Theme.itemSizeExtraLarge))
            cellWidth: Math.floor(width / columns)
            cellHeight: cellWidth

            header: PageHeader {
                title: Media.kindName(page.kind)
            }

            // Silica's grid cell with menu: long press opens it under the row.
            delegate: GridItem {
                id: tile
                objectName: "mediaTile" + model.message_id
                contentWidth: grid.cellWidth
                contentHeight: grid.cellHeight

                readonly property bool isVideo: model.view_type === "Video"
                readonly property bool isAnimated: model.view_type === "Gif"
                /// The file, once row loaded.
                readonly property url fileUrl:
                    model.loaded ? page.fileUrlOf(model.file_path) : ""
                /// Half the gap between tiles.
                readonly property real inset: Theme.paddingSmall / 2
                /// Picture on its way out.
                readonly property bool doomed:
                    doomedMessages.pending(model.message_id)
                /// Countdown already up for this wait; see `page.raiseRemorse`.
                property bool countingDown: false

                Loader {
                    id: remorse
                    active: false
                    sourceComponent: RemorseItem {
                        objectName: "mediaRemorse"
                        onCanceled: doomedMessages.spare(model.message_id)
                    }
                }

                // Wait asked on page, starts while menu gone: tile hears here.
                onDoomedChanged: {
                    if (tile.doomed) {
                        page.raiseRemorse(tile, remorse, body, model.message_id)
                    } else {
                        tile.countingDown = false
                    }
                }

                // Rebuilt on scroll-back: loses countdown. Re-show it.
                Component.onCompleted: {
                    if (tile.doomed) {
                        page.raiseRemorse(tile, remorse, body, model.message_id)
                    }
                }

                // Component, built on first open. See ConversationList.
                menu: Component {
                    ContextMenu {
                        MenuItem {
                            objectName: "showItem"
                            //: Goes back to the conversation, at this message.
                            text: qsTr("Show in chat")
                            onClicked: page.showInChat(model.message_id)
                        }
                        MenuItem {
                            objectName: "deleteItem"
                            text: qsTr("Delete")
                            // Page told, not tile: wait must outlive it (see
                            // PendingRemoval). Delete kind: DeleteMessageDialog.
                            onClicked: page.askDelete(
                                           model.message_id,
                                           model.is_outgoing && model.show_padlock
                                           && !model.is_info)
                        }
                    }
                }

                // Drawn in cell, so countdown covers it: faded, out of tap's
                // reach.
                Item {
                    id: body
                    width: tile.contentWidth
                    height: tile.contentHeight
                    enabled: !tile.doomed

                    // Shows until thumbnailer draws: row not loaded, or file
                    // not reached.
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: tile.inset
                        color: Theme.rgba(Theme.highlightDimmerColor, 0.4)
                    }

                    // Platform thumbnailer: cell-sized, cached. No full decode
                    // per tile.
                    Thumbnail {
                        objectName: "tileThumbnail"
                        anchors.fill: parent
                        anchors.margins: tile.inset
                        sourceSize.width: width
                        sourceSize.height: height
                        // Type told: thumbnailer picks its reader by it. No
                        // core type: assume tile's kind.
                        mimeType: model.file_mime.length > 0 ? model.file_mime
                                  : tile.isVideo ? "video/mp4" : "image/jpeg"
                        source: tile.fileUrl
                    }

                    // Marks conversation draws for same two kinds.
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
                            // Play mark, format name: not to translate.
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
                /// The file, once row loaded.
                readonly property url fileUrl:
                    model.loaded ? page.fileUrlOf(model.file_path) : ""
                /// Message on its way out.
                readonly property bool doomed:
                    doomedMessages.pending(model.message_id)
                /// Countdown already up. See the tile.
                property bool countingDown: false

                // Wait starts on page menu leads to. See tile.
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

                // Rebuilt mid-wait: comes back bare. See tile.
                Component.onCompleted: {
                    if (row.doomed) {
                        page.raiseRemorse(row, remorse, content, model.message_id)
                    }
                }

                // Built when first opened. See tile.
                menu: Component {
                    ContextMenu {
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
                            onClicked: saver.keep(row.fileUrl, model.file_name)
                        }
                        MenuItem {
                            objectName: "deleteItem"
                            text: qsTr("Delete")
                            // Page told, not row. See tile.
                            onClicked: page.askDelete(
                                           model.message_id,
                                           model.is_outgoing && model.show_padlock
                                           && !model.is_info)
                        }
                    }
                }

                // Caption and row together: countdown covers both.
                Item {
                    id: content
                    width: parent.width
                    height: body.y + body.height + Theme.paddingMedium
                    enabled: !row.doomed

                    // Sender and time. Name is theirs.
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

                    // Two row shapes, zero-high unless showing; height is their
                    // sum.
                    Item {
                        id: body
                        x: Theme.horizontalPageMargin
                        y: caption.y + caption.height + Theme.paddingSmall
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        height: fileRow.height + preview.height

                        // File: theme icon, name, size. Like platform file
                        // browser.
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

                        // Sound or app: conversation's own rendering. Gets no
                        // file for a file (row above draws it): measures 0.
                        AttachmentPreview {
                            id: preview
                            objectName: "attachment"
                            y: fileRow.height
                            contentWidth: parent.width
                            filePath: row.isFile || !model.loaded ? "" : model.file_path
                            fileName: model.file_name
                            fileMime: model.file_mime
                            fileBytes: model.file_bytes
                            durationMs: model.duration_ms
                            viewType: row.isFile ? "Text" : model.view_type
                            webxdcName: model.webxdc_name
                            webxdcDocument: model.webxdc_document
                            webxdcSummary: model.webxdc_summary
                            webxdcIcon: model.webxdc_icon
                            // Apps page only offered where apps on, so row
                            // draws as the app.
                            appsEnabled: true
                            onMenuRequested: row.openMenu()
                        }
                    }
                }

                // Opens and runs as in chat. Sound has own play button.
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
