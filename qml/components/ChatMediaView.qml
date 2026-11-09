// 2.5 not 2.0: Image.autoTransform. See AttachmentPreview.qml.
import QtQuick 2.5
import Sailfish.Silica 1.0
import Nemo.Thumbnailer 1.0
import "../js/Media.js" as Media
import "../js/Format.js" as Format
import Piirit 1.0

/*
 * One chat media kind: grid (pictures, videos; thumbnailer cells, play mark) or list
 * (rest: conversation's attachment rows, sender and time above, age sections). Long
 * press: go to message, delete; files also copy, open with. Delete waits here
 * (PendingRemoval), flushed by page on leave. Newest first (chat_media.rs).
 */
Item {
    id: view

    property int accountId
    property int chatId
    /// Gallery, audio, files or apps (ChatMedia.kind).
    property string kind: "gallery"
    /// Page this sits on: "Show in chat" goes down stack from it.
    property Item host: null
    /// List's header: PageHeader, or room for tab strip.
    property Component header: null
    /// Grid or list, once built: tab strip follows its scroll.
    readonly property Item flickable: loader.item

    /// Page should say this (saved, copied).
    signal noticed(string text)
    /// Core or file error.
    signal failed(string message)

    readonly property bool isGallery: view.kind === "gallery"
    property alias media: media
    property alias doomed: doomedMessages
    property alias saver: saver
    /// Ms before message goes. Tests set low.
    property alias pendingDelay: doomedMessages.delay

    /// Leaving: waits end now, deletes go (page calls on deactivate).
    function flushDeletes() {
        doomedMessages.flush()
    }

    /// App-wide `Transcriber`; null (tests, loaded alone): no transcripts.
    property QtObject transcriber: typeof appWindow !== "undefined" ? appWindow.transcriber : null
    /// See AttachmentPreview.
    property Component transcriptMaker: null
    /// Audio with transcription on.
    readonly property bool transcribes: view.kind === "audio" && view.transcriber !== null
                                        && view.transcriber.enabled === true
    /// Chat's voice messages without transcript, as `count_chat` last answered.
    property int untranscribed: 0

    /// Ask count again; answer by `chat_counted`.
    function countUntranscribed() {
        if (view.transcribes) {
            view.transcriber.count_chat(view.accountId, view.chatId)
        } else {
            view.untranscribed = 0
        }
    }

    onTranscribesChanged: view.countUntranscribed()
    Component.onCompleted: view.countUntranscribed()

    Connections {
        target: view.transcriber
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onChat_counted: {
            if (account_id === view.accountId && chat_id === view.chatId) {
                view.untranscribed = count
            }
        }
        // Transcript made or deleted: count moved.
        onStore_changed: view.countUntranscribed()
    }

    ChatMedia {
        id: media
        objectName: "media"
        account_id: view.accountId
        chat_id: view.chatId
        kind: view.kind
        onError: view.failed(message)
        // Arrival or delete: voice messages counted again.
        onRows_changed: view.countUntranscribed()
    }

    Connections {
        target: core
        // Model re-reads on arrival or delete; kept rows don't move.
        onCore_event: media.handle_event(context_id, kind, payload_json)
        // Model made before core up: nothing to load.
        onStatus_changed: {
            if (core.status === "ready") {
                media.reload()
            }
        }
    }

    /// Messages awaiting delete. Beside views, not rows: deleting destroys rows,
    /// every event reloads them. See PendingRemoval.
    PendingRemoval {
        id: doomedMessages
        // `tag`: DeleteMessageDialog's kind; true asks other ends too.
        onRemove: {
            if (tag === true) {
                media.delete_message_for_all(id)
            } else {
                media.delete_message(id)
            }
        }
    }

    // Copy under sender's name, as conversation row menu. See AttachmentSaver.
    AttachmentSaver {
        id: saver
        objectName: "saver"
        onSaved: view.noticed(saver.savedText)
        onError: view.failed(message)
    }

    /// Ask delete kind in conversation's dialog; wait starts on accept. Gate read
    /// now: row may be gone by answer.
    function askDelete(messageId, canDeleteForEveryone) {
        var chooser = pageStack.push(
            Qt.resolvedUrl("../pages/DeleteMessageDialog.qml"),
            { canDeleteForEveryone: canDeleteForEveryone })
        if (chooser) {
            chooser.picked.connect(function(forEveryone) {
                doomedMessages.ask(messageId, forEveryone)
            })
        }
    }

    /// Silica's countdown over item, built on first ask (row `loader`). Deletion is
    /// `doomedMessages`'s (outlives row): no-op callback, only draws, reports tap.
    function raiseRemorse(cell, loader, over, messageId) {
        // Once per wait (start and rebuild both land here): second `execute` restarts.
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

    /// Path as URL views load; empty until row loaded.
    function fileUrlOf(path) {
        return path.length > 0 ? Qt.resolvedUrl(Media.fileUrl(path)) : ""
    }

    /// Tap opens per kind (ConversationPage.openAttachment): pictures, videos swiped in grid
    /// order (GalleryPage); files to other apps. `held`: not yet downloaded picture or video, no
    /// file: viewer offers Download.
    function open(fileUrl, fileName, viewType, messageId, held) {
        // Row not loaded yet: no file.
        if (String(fileUrl).length === 0 && held !== true) {
            return
        }
        if (viewType === "Image" || viewType === "Gif" || viewType === "Video") {
            pageStack.push(Qt.resolvedUrl("../pages/GalleryPage.qml"), {
                accountId: view.accountId,
                chatId: view.chatId,
                messageId: messageId,
                fileUrl: fileUrl,
                fileName: fileName,
                viewType: viewType,
                newestFirst: true
            })
        } else {
            Qt.openUrlExternally(fileUrl)
        }
    }

    /// Webxdc app, run here. See ConversationPage.openApp.
    function openApp(messageId) {
        pageStack.push(Qt.resolvedUrl("../pages/WebxdcPage.qml"), {
            accountId: view.accountId,
            messageId: messageId
        })
    }

    /// Back to conversation at this message: down stack to first page with
    /// `showMessage`. Told first, popped to second: acts once on screen, over
    /// restored place (ConversationPage.showMessage).
    function showInChat(messageId) {
        var below = pageStack.previousPage(view.host)
        while (below && typeof below.showMessage !== "function") {
            below = pageStack.previousPage(below)
        }
        if (!below) {
            return
        }
        below.showMessage(messageId)
        pageStack.pop(below)
    }

    // Loader builds only shown view: hidden one would still build delegates.
    Loader {
        id: loader
        anchors.fill: parent
        sourceComponent: view.isGallery ? gallery : listing
    }

    // Only while filling: filling list shows it itself.
    BusyIndicator {
        objectName: "mediaBusy"
        anchors.centerIn: parent
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
            model: view.media.rows

            /// Columns: platform largest items that fit; three upright.
            readonly property int columns:
                Math.max(1, Math.floor(width / Theme.itemSizeExtraLarge))
            cellWidth: Math.floor(width / columns)
            cellHeight: cellWidth

            header: view.header

            // Silica grid cell with menu: long press opens it under row.
            delegate: GridItem {
                id: tile
                objectName: "mediaTile" + model.message_id
                contentWidth: grid.cellWidth
                contentHeight: grid.cellHeight

                readonly property bool isVideo: model.view_type === "Video"
                readonly property bool isAnimated: model.view_type === "Gif"
                /// Not yet downloaded: no picture; download mark, size (chat_media.rs).
                readonly property bool held: model.loaded && model.download_state.length > 0
                                             && model.download_state !== "Done"
                /// The file, once row loaded.
                readonly property url fileUrl:
                    model.loaded ? view.fileUrlOf(model.file_path) : ""
                /// Half the gap between tiles.
                readonly property real inset: Theme.paddingSmall / 2
                /// Picture on its way out.
                readonly property bool doomed:
                    view.doomed.pending(model.message_id)
                /// Countdown already up for this wait (`view.raiseRemorse`).
                property bool countingDown: false

                Loader {
                    id: remorse
                    active: false
                    sourceComponent: RemorseItem {
                        objectName: "mediaRemorse"
                        onCanceled: view.doomed.spare(model.message_id)
                    }
                }

                // Page starts wait after menu closes: tile hears here.
                onDoomedChanged: {
                    if (tile.doomed) {
                        view.raiseRemorse(tile, remorse, body, model.message_id)
                    } else {
                        tile.countingDown = false
                    }
                }

                // Rebuilt on scroll-back loses countdown: re-show.
                Component.onCompleted: {
                    if (tile.doomed) {
                        view.raiseRemorse(tile, remorse, body, model.message_id)
                    }
                }

                // Built on first open (ConversationList).
                menu: Component {
                    ContextMenu {
                        MenuItem {
                            objectName: "showItem"
                            //: Goes back to the conversation, at this message.
                            text: qsTr("Show in chat")
                            onClicked: view.showInChat(model.message_id)
                        }
                        MenuItem {
                            objectName: "deleteItem"
                            text: qsTr("Delete")
                            // Page told, not tile: wait outlives it (PendingRemoval). Kind:
                            // DeleteMessageDialog.
                            onClicked: view.askDelete(
                                           model.message_id,
                                           model.is_outgoing && model.show_padlock
                                           && !model.is_info)
                        }
                    }
                }

                // In cell, so countdown covers it: faded, untappable.
                Item {
                    id: body
                    width: tile.contentWidth
                    height: tile.contentHeight
                    enabled: !tile.doomed

                    // Until thumbnailer draws: row not loaded, or file not reached.
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: tile.inset
                        color: Theme.rgba(Theme.highlightDimmerColor, 0.4)
                    }

                    // Platform thumbnailer: cell-sized, cached; no full decode per tile.
                    Thumbnail {
                        objectName: "tileThumbnail"
                        anchors.fill: parent
                        anchors.margins: tile.inset
                        sourceSize.width: width
                        sourceSize.height: height
                        // Type told: thumbnailer picks reader by it. No core type: tile's kind.
                        mimeType: model.file_mime.length > 0 ? model.file_mime
                                  : tile.isVideo ? "video/mp4" : "image/jpeg"
                        source: tile.fileUrl
                    }

                    // Conversation's marks for same two kinds.
                    Rectangle {
                        objectName: "playMark"
                        visible: (tile.isVideo || tile.isAnimated) && !tile.held
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

                    // Not yet downloaded: conversation's download arrow, size to fetch.
                    Column {
                        objectName: "heldMark"
                        visible: tile.held
                        anchors.centerIn: parent
                        width: parent.width - 2 * Theme.paddingSmall

                        Label {
                            anchors.horizontalCenter: parent.horizontalCenter
                            color: Theme.highlightColor
                            font.pixelSize: Theme.fontSizeLarge
                            textFormat: Text.PlainText
                            // Conversation's Download mark: not to translate.
                            text: "⬇"
                        }
                        Label {
                            objectName: "heldSize"
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            color: Theme.highlightColor
                            font.pixelSize: Theme.fontSizeExtraSmall
                            truncationMode: TruncationMode.Fade
                            textFormat: Text.PlainText
                            text: tile.held ? Format.readableSize(model.file_bytes) : ""
                        }
                    }
                }

                onClicked: view.open(tile.fileUrl, model.file_name, model.view_type,
                                     model.message_id, tile.held)
            }

            ViewPlaceholder {
                objectName: "placeholder"
                enabled: view.media.loaded && view.media.count === 0
                text: Media.emptyText(view.kind)
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
            model: view.media.rows

            header: view.header

            // Age sections, as Salama's history: heading in first row of each (`starts_group`).
            // Not `section`: keeps "0" a row had before it was read.

            // Audio, transcription on, something left to do.
            PullDownMenu {
                objectName: "transcribePulley"
                visible: view.transcribes && view.untranscribed > 0

                MenuItem {
                    objectName: "transcribeChatItem"
                    //: Pulley on a chat's Audio view. %n is how many voice
                    //: messages have no transcript yet.
                    text: qsTr("Transcribe %n voice message(s)", "", view.untranscribed)
                    onClicked: {
                        view.transcriber.transcribe_chat(view.accountId, view.chatId)
                        view.countUntranscribed()
                    }
                }
            }

            delegate: ListItem {
                id: row
                objectName: "mediaRow" + model.message_id
                width: list.width
                contentHeight: ageHeading.height + content.height

                /// Own words ("Today"), not the other end's: SectionHeader can't pin plain text.
                readonly property string ageName:
                    model.starts_group ? Media.dateGroupName(model.date_group) : ""
                readonly property bool isFile: view.kind === "files"
                readonly property bool isApp: view.kind === "apps"
                /// The file, once row loaded.
                readonly property url fileUrl:
                    model.loaded ? view.fileUrlOf(model.file_path) : ""
                /// Message on its way out.
                readonly property bool doomed:
                    view.doomed.pending(model.message_id)
                /// Countdown already up; see tile.
                property bool countingDown: false

                // Wait starts on page menu leads to; see tile.
                onDoomedChanged: {
                    if (row.doomed) {
                        view.raiseRemorse(row, remorse, content, model.message_id)
                    } else {
                        row.countingDown = false
                    }
                }

                Loader {
                    id: remorse
                    active: false
                    sourceComponent: RemorseItem {
                        objectName: "mediaRemorse"
                        onCanceled: view.doomed.spare(model.message_id)
                    }
                }

                // Rebuilt mid-wait comes back bare; see tile.
                Component.onCompleted: {
                    if (row.doomed) {
                        view.raiseRemorse(row, remorse, content, model.message_id)
                    }
                }

                // Built on first open; see tile.
                menu: Component {
                    ContextMenu {
                        MenuItem {
                            objectName: "showItem"
                            //: Goes back to the conversation, at this message.
                            text: qsTr("Show in chat")
                            onClicked: view.showInChat(model.message_id)
                        }
                        MenuItem {
                            objectName: "copyTranscriptItem"
                            // 3 = `Transcript` done.
                            visible: preview.transcript !== null
                                     && preview.transcript.state === 3
                            //: Copies the text a voice message was turned into.
                            text: qsTr("Copy transcript")
                            onClicked: {
                                Clipboard.text = preview.transcript.text
                                view.noticed(qsTr("Copied to clipboard"))
                            }
                        }
                        MenuItem {
                            objectName: "transcribeItem"
                            // As conversation's row menu.
                            visible: preview.transcript !== null
                                     && (preview.transcript.state === 0
                                         || preview.transcript.state === 6
                                         || (preview.transcript.state === 1
                                             && !preview.transcript.manual))
                            //: Turns a voice message into text on the phone.
                            text: qsTr("Transcribe")
                            onClicked: preview.transcript.transcribe(false)
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
                            onClicked: view.saver.keep(row.fileUrl, model.file_name)
                        }
                        MenuItem {
                            objectName: "deleteItem"
                            text: qsTr("Delete")
                            // Page told, not row; see tile.
                            onClicked: view.askDelete(
                                           model.message_id,
                                           model.is_outgoing && model.show_padlock
                                           && !model.is_info)
                        }
                    }
                }

                // Over the row, not under the countdown.
                SectionHeader {
                    id: ageHeading
                    objectName: "dateSection"
                    visible: model.starts_group
                    height: model.starts_group ? implicitHeight : 0
                    text: row.ageName
                }

                // Caption and row together: countdown covers both.
                Item {
                    id: content
                    y: ageHeading.height
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
                        // Today, yesterday: section says day, row time. Older: date.
                        text: model.loaded
                              ? model.sender_name + " · "
                                + (model.date_group <= 2
                                   ? Qt.formatTime(new Date(model.timestamp * 1000),
                                                   Qt.DefaultLocaleShortDate)
                                   : Qt.formatDate(new Date(model.timestamp * 1000),
                                                   Qt.DefaultLocaleShortDate))
                              : ""
                    }

                    // Two row shapes, zero-high unless shown; height is sum.
                    Item {
                        id: body
                        x: Theme.horizontalPageMargin
                        y: caption.y + caption.height + Theme.paddingSmall
                        width: parent.width - 2 * Theme.horizontalPageMargin
                        height: fileRow.height + preview.height

                        // File: theme icon, name, size, as platform file browser.
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

                        // Sound or app: conversation's rendering. No file for a file (row above
                        // draws it): measures 0.
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
                            // Apps page only where apps on: row draws as app.
                            appsEnabled: true
                            accountId: view.accountId
                            messageId: model.message_id
                            transcriber: view.transcriber
                            transcriptMaker: view.transcriptMaker
                            compactTranscript: true
                            onMenuRequested: row.openMenu()
                        }
                    }
                }

                // Opens and runs as in chat. Sound has own play button.
                onClicked: {
                    if (row.isApp) {
                        view.openApp(model.message_id)
                    } else if (row.isFile) {
                        view.open(row.fileUrl, model.file_name, model.view_type,
                                  model.message_id)
                    }
                }
            }

            ViewPlaceholder {
                objectName: "placeholder"
                enabled: view.media.loaded && view.media.count === 0
                text: Media.emptyText(view.kind)
            }

            VerticalScrollDecorator {}
        }
    }

}
