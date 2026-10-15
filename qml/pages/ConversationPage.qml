import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Format.js" as Format
import "../js/Emoji.js" as Emoji
import Piirit 1.0

/*
 * One conversation. Own ChatMessages: another open one can't reset its model.
 */
Page {
    id: page

    property int accountId
    property int chatId
    property string chatName
    /// Search-found message to open at. 0: newest.
    property int findMessageId: 0

    ChatMessages {
        id: messages
        objectName: "messages"
        account_id: page.accountId
        // Visible messages decide read.
        reading_history: !page.readerIsLooking
        // Reader setting: strip tracking params from links.
        clean_links: Settings.cleanLinks === true
        // Outgoing media quality: video shrunk to this before send.
        media_quality: Settings.mediaQuality === 1 ? 1 : 0
        // Bar file, weighed against core's recommended size. None while editing.
        pending_file: page.editing ? "" : page.attachmentPath
        onError: {
            page.errorMessage = message
            // Unsent voice message: discard, else lingers on phone.
            page.dropVoice()
            // Refused file stays on bar, not retried.
            if (page.sendingOnItsOwn) {
                page.sendingOnItsOwn = false
                page.sendWhenReady = false
            }
        }
        // Sending means read: go to sent message.
        onSent: {
            // File sent alone (`sendWhenReady`) keeps field, reply, draft.
            var onItsOwn = page.sendingOnItsOwn
            page.sendingOnItsOwn = false
            if (!onItsOwn) {
                textField.text = ""
                // Now: else chat list shows sent message as draft.
                page.storeDraft()
                page.replyBody = ""
                page.replyAuthor = ""
            }
            // Core copied capture; library file stays.
            captures.discard(page.attachmentPath)
            page.attachmentPath = ""
            page.dropVoice()
            page.sendWhenReady = false
            listView.jumpToNewest()
        }
        // `sendWhenReady` waits on these.
        onAttachment_limit_changed: sharedSend.restart()
        onPreparing_changed: sharedSend.restart()
        onLoaded_changed: sharedSend.restart()
        // Edit is not a send: field restored after.
        onEdited: page.finishEdit()
        onArrived: listView.noteArrivals(count)
        // Unsent text, once core answers. `draftApplied` only when text lands:
        // handover's clear-and-signal isn't the answer (draft never came back).
        onDraft_changed: {
            if (!page.draftApplied && messages.draft.length > 0
                    && textField.text.length === 0) {
                textField.text = messages.draft
                page.draftApplied = true
            }
        }
    }

    // Handed over as page is built, not on Active: one page of fifty, usually
    // prefetched, no long Qt-thread build. In `Component.onCompleted`: after
    // `reading_history` bound, else default `false` marks chat read early.
    /// Share that opened chat: file for bar, text for field (qml/share/ShareTarget.qml,
    /// ChatListPage.capture).
    property string sharedFile: ""
    property string sharedText: ""
    /// Shared file sent without send button (camera quick action "send at
    /// once"). Read at creation.
    property bool sendShared: false

    Component.onCompleted: {
        messages.chat_id = page.chatId
        // Before draft arrives, which fills only an empty field: draft kept.
        if (page.sharedFile.length > 0) {
            page.attach(page.sharedFile)
            page.sendWhenReady = page.sendShared
        }
        if (page.sharedText.length > 0) {
            textField.text = page.sharedText
        }
    }

    /// Shared file sent once size limit known, shrink done (else full size),
    /// rows in (else lands on replaced rows, missing till reopen). Cleared once
    /// sent, refused or taken over (other file, shrink stopped, reply/edit, send).
    property bool sendWhenReady: false
    /// Send under way is that one, not reader's.
    property bool sendingOnItsOwn: false

    // One event turn after model moved: mid-change it can't take send.
    // File alone: field text is draft, not caption. Reply/edit begun: file waits.
    Timer {
        id: sharedSend
        interval: 0
        onTriggered: {
            if (!page.sendWhenReady || !messages.loaded || messages.preparing
                    || messages.sending || page.attachmentPath.length === 0) {
                return
            }
            if (page.editing || messages.quoted_message_id !== 0) {
                page.sendWhenReady = false
                return
            }
            page.errorMessage = ""
            page.sendingOnItsOwn = true
            messages.send_file("", page.attachmentPath)
        }
    }

    // List place held while away, restored to pixel.
    onStatusChanged: {
        if (page.status === PageStatus.Deactivating) {
            listView.rememberPlace()
            // Now: debounce may not have fired.
            page.storeDraft()
            // Close tray, else it reopens on return over picked file.
            attachButton.close()
            // Deletes asked for go now, same reason as draft.
            listView.flushDeletes()
        } else if (page.status === PageStatus.Active) {
            listView.restorePlace()
            // Asked while away (media page): lands over restored place.
            page.revealIfAsked()
            page.attachInfo()
        }
    }

    /// Go to one message: search result, or media page's ask on return
    /// (ChatMediaPage.showInChat). Off screen: kept, lands over restored place.
    function showMessage(messageId) {
        page.findMessageId = messageId
        if (page.status === PageStatus.Active) {
            page.revealIfAsked()
        }
    }

    /// Ask model where message is, once rows in; answer in `onRevealed`.
    function revealIfAsked() {
        if (page.findMessageId !== 0 && messages.loaded) {
            messages.reveal(page.findMessageId)
        }
    }

    /// Chat's draft in field yet. Core answers late; mustn't overwrite typing.
    property bool draftApplied: false

    function storeDraft() {
        draftDebounce.stop()
        // Not while field holds sent message: core's copy is unsent words.
        if (page.editing) {
            return
        }
        messages.save_draft(textField.text)
    }

    // Not per keystroke: one core call per character.
    Timer {
        id: draftDebounce
        interval: 1000
        onTriggered: page.storeDraft()
    }

    /// Own message being edited; 0 none. Field mode: its text, bar says so, no
    /// tray, send sends change (as deltachat-android/ios).
    property int editingMessageId: 0
    /// Text as sent, for bar.
    property string editingBody: ""
    /// Unsent draft before edit, restored on send or cancel (reference clients
    /// discard it; deliberate).
    property string stashedDraft: ""
    readonly property bool editing: page.editingMessageId !== 0

    function beginEdit(messageId, body) {
        // Second Edit keeps first stash: field holds first message's text.
        if (!page.editing) {
            page.stashedDraft = textField.text
        }
        page.editingMessageId = messageId
        page.editingBody = body
        textField.text = body
        textField.forceActiveFocus()
    }

    function cancelEdit() {
        page.editingMessageId = 0
        page.editingBody = ""
        textField.text = page.stashedDraft
        page.stashedDraft = ""
    }

    /// Edit reached core: field as on cancel.
    function finishEdit() {
        page.cancelEdit()
    }

    // Search result: waits for model, not page. `reveal` steps back until
    // found message is loaded.
    Connections {
        target: messages
        onLoaded_changed: {
            if (messages.loaded && page.findMessageId !== 0) {
                messages.reveal(page.findMessageId)
            }
        }
        onRevealed: {
            if (row >= 0) {
                listView.foundMessageId = message_id
                // Held: page arriving, rows measuring; jump lands where measuring got.
                listView.holdAt(row)
                foundFlash.restart()
            }
            // Once: later reload mustn't pull reader back.
            page.findMessageId = 0
        }
        // Page at a time; asks again on finish until nothing left.
        onHydrating_changed: if (!messages.hydrating) listView.askForRows()
    }

    // Flash, then gone.
    Timer {
        id: foundFlash
        interval: 4000
        onTriggered: listView.foundMessageId = 0
    }

    // Seen only if app in front, page on screen, view at newest, no drag (else
    // read receipt lies). `following`: `stickToBottom` stale during drag.
    readonly property bool readerIsLooking:
        Qt.application.state === Qt.ApplicationActive
        && page.status === PageStatus.Active
        && listView.following

    property string errorMessage: ""
    // Core going away is handled: no restart banner.
    readonly property string coreStatusMessage:
        core.status === "reconnecting"
        ? qsTr("Lost the connection to the Delta Chat core. Reconnecting…")
        : core.status === "stopped"
          ? qsTr("Lost the connection to the Delta Chat core. Restart Piirit.")
          : ""

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core
        // Model ignores other accounts'/chats' events.
        onCore_event: messages.handle_event(context_id, kind, payload_json)
        // Before core is up: nothing to load.
        onStatus_changed: {
            if (core.status === "ready") {
                messages.reload()
            }
        }
        onCore_error: page.errorMessage = message
    }

    // Reply target for bar; id lives on model.
    property string replyBody: ""
    property string replyAuthor: ""

    function cancelReply() {
        messages.quoted_message_id = 0
        page.replyBody = ""
        page.replyAuthor = ""
    }

    // File next send carries, one per message (ChatMessages.send_file).
    property string attachmentPath: ""
    readonly property string attachmentName: page.displayName(page.attachmentPath)

    // Decoded as shim decodes sent path (picker URLs escape spaces).
    // Undecodable escape: literal '%' in name.
    function displayName(path) {
        var name = path.substring(path.lastIndexOf('/') + 1)
        try {
            return decodeURIComponent(name)
        } catch (error) {
            return name
        }
    }

    // Pickers report back, not send: file cancellable, caption can follow.
    function attach(path) {
        if (path && path.length > 0) {
            // Capture replaced before send goes nowhere.
            captures.discard(page.attachmentPath)
            page.attachmentPath = path
            // Only shared file goes on its own.
            page.sendWhenReady = false
        }
    }

    // Photos and voice messages made here; page discards after.
    Captures {
        id: captures
        objectName: "captures"
    }

    // Cancel discards capture, leaves picked file.
    function dropAttachment() {
        captures.discard(page.attachmentPath)
        page.attachmentPath = ""
    }

    /// Voice message on way to core until answered; not via bar, so kept here.
    property string pendingVoice: ""

    function dropVoice() {
        captures.discard(page.pendingVoice)
        page.pendingVoice = ""
    }

    // Capture left on bar, or unanswered voice message: discarded (nothing else
    // knows of it), unless a send is in flight (sweep in capture.rs handles
    // that). Recoded video is model's to remove.
    Component.onDestruction: {
        if (!messages.sending) {
            page.dropAttachment()
            page.dropVoice()
        }
    }

    // By URL like ChatPickerPage: only pickers name `Sailfish.Pickers`, only
    // capture page and scanner Camera; missing type costs one button.
    function pickWith(pageName) {
        var picker = pageStack.push(Qt.resolvedUrl(pageName))
        // Null when page failed to load: connecting would throw.
        if (picker) {
            picker.picked.connect(page.attach)
        }
    }

    // Outside list (would scroll off). Not PageHeader: can't show other end's
    // chosen name as plain text.
    ConversationHeader {
        id: conversationHeader
        objectName: "conversationHeader"
        title: page.chatName
        // Member count. None for one-to-one ("2") or before chat shape arrives
        // ("0 members" flicker).
        //: Under a group's name, over its messages. %n is how many
        //: people are in the group.
        subtitle: messages.is_group && messages.member_count > 0
                  ? qsTr("%n member(s)", "", messages.member_count)
                  : ""
        // White once info page attached, as navigable PageHeader.
        interactive: page.canNavigateForward
        // Name goes swipe's way.
        onClicked: page.openInfo()
    }

    /// Call this chat, or bring existing call to front. One call at a time (as
    /// reference clients).
    function placeCall(video) {
        if (typeof appWindow === "undefined") {
            return
        }
        if (!appWindow.placeCall(page.accountId, page.chatId, video === true)) {
            appWindow.showCall()
        }
    }

    /// Call row tapped: ringing call answered (may have rung unheard); else
    /// called back as made (video: camera on). As reference clients.
    function callFromRow(messageId, outgoing, callState, hasVideo) {
        if (typeof appWindow === "undefined") {
            return
        }
        if (!outgoing && callState === "Alerting") {
            appWindow.pickUpCall(page.accountId, page.chatId, messageId)
            return
        }
        if (messages.can_call) {
            page.placeCall(hasVideo)
        }
    }

    // Group or contact info, right; waits for chat kind. Attached (indicator,
    // swipe reach it); reattached each return, a pushed page takes it along.
    function attachInfo() {
        if (!messages.loaded || page.status !== PageStatus.Active) {
            return
        }
        if (pageStack.nextPage(page)) {
            return
        }
        pageStack.pushAttached(
            Qt.resolvedUrl(messages.is_group ? "GroupPage.qml" : "ContactPage.qml"), {
                accountId: page.accountId,
                chatId: page.chatId,
                chatName: page.chatName
            })
    }

    function openInfo() {
        if (pageStack.nextPage(page)) {
            pageStack.navigateForward()
        }
    }

    Connections {
        target: messages
        onLoaded_changed: page.attachInfo()
        // Name from list until core says; later renames via model.
        onChat_name_changed: {
            if (messages.chat_name.length > 0) {
                page.chatName = messages.chat_name
            }
        }
    }

    ConversationList {
        loaded: messages.loaded
        id: listView
        objectName: "messageList"
        anchors {
            top: conversationHeader.bottom
            left: parent.left
            right: parent.right
            bottom: banner.top
        }
        model: messages.rows
        // Model count: changes on arrival, not on view.
        messageCount: messages.count
        // Which rows are on screen, for model to fill.
        onHydrateRequested: messages.hydrate(first, last)
        showSender: messages.is_group
        // Last-read place, from core on open.
        unreadFrom: messages.unread_from
        markdownMode: Settings.markdownMode
        // Off: `.xdc` is a file. `=== true`: dconf undefined before read.
        appsEnabled: Settings.webxdcEnabled === true
        // What core takes an edit in: see model.
        canEdit: messages.can_send && messages.is_encrypted
        placeholderText: qsTr("No messages yet")

        // Reaching newest marks what is there read.
        onArrivedAtNewest: messages.mark_seen_all()
        onReplyRequested: {
            messages.quoted_message_id = messageId
            page.replyBody = body
            page.replyAuthor = author
        }
        onEditRequested: page.beginEdit(messageId, body)
        onCopyRequested: {
            Clipboard.text = body
            notice.show(qsTr("Copied to clipboard"))
        }
        onOpenRequested: page.openAttachment(fileUrl, fileName, viewType,
                                             previewWidth)
        onSaveRequested: attachmentSaver.keep(fileUrl, fileName)
        onFullTextRequested: pageStack.push(Qt.resolvedUrl("MessagePage.qml"), {
            accountId: page.accountId,
            messageId: messageId,
            senderName: author,
            markdownMode: Settings.markdownMode
        })
        onAppRequested: page.openApp(messageId)
        onCallRequested: page.callFromRow(messageId, outgoing, callState, hasVideo)
        onDownloadRequested: messages.download_full(messageId)
        // On/off is model's call (knows what reader sent); core takes whole list.
        onReactionRequested: messages.react(messageId, emoji)
        // Other emoji: page; id captured now (row may go).
        onReactionPickerRequested: {
            var reactingTo = messageId
            var mine = current
            var picker = pageStack.push(
                Qt.resolvedUrl("ReactionPickerPage.qml"),
                { current: mine, recent: Settings.recentReactions })
            if (picker) {
                picker.picked.connect(function(emoji) {
                    messages.react(reactingTo, emoji)
                    // Own reaction not offered again.
                    if (emoji !== mine) {
                        Settings.recentReactions = Emoji.remember(
                            Settings.recentReactions, emoji)
                    }
                })
            }
        }
        // Delete kind (this account or everybody) picked on menu's page.
        onDeleteRequested: {
            if (forEveryone) {
                messages.delete_message_for_all(messageId)
            } else {
                messages.delete_message(messageId)
            }
        }
        // Page pushes dialog; nothing deleted until accepted and wait done
        // (back edge deletes nothing).
        onDeleteChoiceRequested: {
            // Taken now: row may be gone by callback (as Forward).
            var doomed = messageId
            var chooser = pageStack.push(
                Qt.resolvedUrl("DeleteMessageDialog.qml"),
                { canDeleteForEveryone: canDeleteForEveryone })
            if (chooser) {
                chooser.picked.connect(function(forEveryone) {
                    listView.confirmDelete(doomed, forEveryone)
                })
            }
        }
        onResendRequested: messages.resend_message(messageId)
        onForwardRequested: {
            // Message id captured here for picker's report.
            var travelling = messageId
            var picker = pageStack.push(
                Qt.resolvedUrl("ChatPickerPage.qml"),
                { accountId: page.accountId })
            if (picker) {
                picker.chatPicked.connect(function(chatId) {
                    messages.forward_to(travelling, chatId)
                })
            }
        }
    }

    // Only when scrolled from newest with arrivals below.
    JumpButton {
        objectName: "jumpButton"
        visible: !listView.stickToBottom && listView.missedCount > 0
        count: listView.missedCount
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
            bottom: banner.top
            bottomMargin: Theme.paddingMedium
        }
        onClicked: listView.jumpToNewest()
    }

    // Between list and field (translucent would show messages).
    Banner {
        id: banner
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
        // Core state outranks earlier failure: read now, don't wait for change.
        text: page.coreStatusMessage.length > 0
              ? page.coreStatusMessage : page.errorMessage
        // Reconnect ends when status says; stopped never does.
        timeout: page.coreStatusMessage.length > 0 ? 0 : 8
        onDismissed: page.errorMessage = ""
    }

    // Reply or edit bar, one for both (edit isn't a send); reply returns after.
    ReplyBar {
        id: replyBar
        objectName: "replyBar"
        anchors {
            left: parent.left
            right: parent.right
            bottom: attachmentBar.top
        }
        editing: page.editing
        author: page.editing ? "" : page.replyAuthor
        body: page.editing ? page.editingBody : page.replyBody
        onCancelled: {
            if (page.editing) {
                page.cancelEdit()
            } else {
                page.cancelReply()
            }
        }
    }

    // Directly above field: part of message being sent.
    AttachmentBar {
        id: attachmentBar
        objectName: "attachmentBar"
        anchors {
            left: parent.left
            right: parent.right
            bottom: largeFileBar.top
        }
        // Hidden while editing (no file); file still here after.
        filePath: page.editing ? "" : page.attachmentPath
        fileName: page.attachmentName
        // Video shrinks as picked; button stops shrink, keeps file; again drops it.
        preparing: messages.preparing
        progress: messages.preparing_progress
        // How much smaller.
        originalBytes: page.editing ? 0 : messages.original_bytes
        bytes: messages.attachment_bytes
        onCancelled: page.dropAttachment()
        // Stopped video goes as is, even if set to send at once.
        onStopped: {
            page.sendWhenReady = false
            messages.cancel_preparing()
        }
    }

    // Over core's recommended size (some relays refuse): warning, send on (one
    // size for all relays, many take more; issue #101). Never pictures (core
    // shrinks); videos at shrink size. See rust/piirit-shim/src/media.rs, video.rs.
    Banner {
        id: largeFileBar
        objectName: "largeFileBar"
        labelObjectName: "largeFileLabel"
        tone: "info"
        // Not transient: true while file on bar.
        timeout: 0
        // Video sent smaller: picked size irrelevant; bar says both.
        text: !messages.attachment_large ? ""
              : messages.original_bytes > 0
              //: Shown above the message field when a video is being
              //: made smaller for sending and is still bigger than Delta
              //: Chat recommends. The file is still sent. %1 is the
              //: recommended largest size, such as "24 MB".
              ? qsTr("Even made smaller, this file is bigger than the %1 most relays accept. Sending may fail.")
                .arg(Format.readableSize(messages.attachment_limit))
              //: Shown above the message field when the attached file is
              //: bigger than Delta Chat recommends. The file is still
              //: sent. %1 is the file's size and %2 the recommended
              //: largest size, each such as "24 MB".
              : qsTr("At %1, this file is bigger than the %2 most relays accept. Sending may fail.")
                .arg(Format.readableSize(messages.attachment_bytes))
                .arg(Format.readableSize(messages.attachment_limit))
        anchors {
            left: parent.left
            right: parent.right
            bottom: inputRow.top
        }
    }

    // What happened where page has no state (clipboard).
    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 4
        anchors {
            left: parent.left
            right: parent.right
            bottom: replyBar.top
        }
        onDismissed: notice.text = ""
    }

    /// File on bar still shrinking: holds send. Not while editing.
    readonly property bool videoPreparing: messages.preparing && !page.editing

    /// What send sends. Editing: text only.
    readonly property bool hasSomethingToSend:
        textField.text.trim().length > 0
        || (!page.editing && page.attachmentPath.length > 0)

    // Tap outside tray closes it, as Silica menus. Declared here: over list and
    // bars, under input row.
    MouseArea {
        objectName: "trayDismiss"
        anchors.fill: parent
        visible: attachButton.open
        onClicked: attachButton.close()
    }

    Row {
        id: inputRow
        objectName: "inputRow"
        anchors {
            left: parent.left
            right: parent.right
            // Field's own left inset; else send button hugs edge.
            rightMargin: Theme.horizontalPageMargin
            bottom: parent.bottom
            // Off screen edge: TextArea has no room under text, recording less.
            bottomMargin: Theme.paddingLarge
        }
        spacing: Theme.paddingSmall

        // Field plus lifted buttons: Row ignores child anchors.
        height: Math.max(textField.visible ? textField.height : 0,
                         voiceBar.visible ? voiceBar.height : 0,
                         sendButton.height + inputRow.buttonLift)

        /// Buttons' lift over field. None: Silica field keeps descender room;
        /// padding lift made them float.
        readonly property real buttonLift: 0

        // Recording, in field's place.
        VoiceBar {
            id: voiceBar
            objectName: "voiceBar"
            width: parent.width - sendButton.width
            anchors.verticalCenter: sendButton.verticalCenter
            // Stops at core's recommended size (over an hour of speech), at
            // media-quality bit rate.
            limitBytes: messages.attachment_limit
            mediaQuality: Settings.mediaQuality === 1 ? 1 : 0
            onRecorded: {
                page.errorMessage = ""
                page.pendingVoice = path
                messages.send_voice(path)
            }
            onFailed: page.errorMessage = message
        }

        // Both hidden while recording (Row lays out visible only). Multi-line:
        // return inserts newline; send is the button.
        TextArea {
            id: textField
            objectName: "messageField"
            visible: !voiceBar.recording
            // Visible buttons only; tray hides on edit.
            width: parent.width - (attachButton.visible ? attachButton.width : 0)
                   - sendButton.width
            // Row bottom: Row lays from top, field would rise with buttons.
            anchors.bottom: parent.bottom
            //: Message field placeholder. Also the prompt for the caption
            //: on a message that is carrying a file.
            placeholderText: page.attachmentPath.length > 0
                             ? qsTr("Caption") : qsTr("Message")
            // Label as placeholder.
            labelVisible: false
            // Grows to a third of screen, then scrolls.
            height: Math.min(implicitHeight, page.height / 3)
            // Kept in core across restarts; chat list shows it.
            onTextChanged: draftDebounce.restart()
            // Typing isn't a tap on tray.
            onActiveFocusChanged: if (activeFocus) attachButton.close()
        }

        // Buttons at row bottom, beside draft's last line.
        AttachButton {
            id: attachButton
            objectName: "attachButton"
            // Not while editing.
            visible: !voiceBar.recording && !page.editing
            anchors.bottom: parent.bottom
            anchors.bottomMargin: inputRow.buttonLift
            voiceAvailable: voiceBar.available
            appsAvailable: Settings.webxdcEnabled === true
            onCameraRequested: page.pickWith("CapturePage.qml")
            onLibraryRequested: page.pickWith("AttachLibraryPage.qml")
            onVoiceRequested: voiceBar.start()
            // `.xdc` sent as file; core makes it an app.
            onAppRequested: page.pickApp()
        }

        IconButton {
            id: sendButton
            objectName: "sendButton"
            anchors.bottom: parent.bottom
            anchors.bottomMargin: inputRow.buttonLift
            // Hidden, not greyed, while sending (keeps row shape). Editing: tick.
            icon.source: messages.sending ? ""
                         : page.editing ? "image://theme/icon-m-accept"
                                        : "image://theme/icon-m-send"
            // File alone is a message; blank is not. Recording: stops and sends.
            // No double send (large video copy lets second tap through).
            // Shrinking video: waits for ready + reader, unless `sendWhenReady`.
            enabled: !messages.sending
                     && (!page.videoPreparing || voiceBar.recording)
                     && (page.hasSomethingToSend || voiceBar.recording)
            onClicked: page.sendCurrentText()

            BusyIndicator {
                objectName: "sendBusy"
                anchors.centerIn: parent
                size: BusyIndicatorSize.Small
                running: messages.sending
            }
        }
    }

    // Pictures and videos shown here (system handover failed to play them).
    // Rest handed on ("cannot show" page is worse). Copy is on row menu.
    function openAttachment(fileUrl, fileName, viewType, previewWidth) {
        if (viewType === "Image" || viewType === "Gif"
                || viewType === "Sticker") {
            pageStack.push(Qt.resolvedUrl("PicturePage.qml"), {
                fileUrl: fileUrl,
                fileName: fileName,
                viewType: viewType,
                // Row's drawn width, if from a row.
                previewWidth: previewWidth > 0 ? previewWidth : 0
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

    // Copy to Piirit's Downloads folder; no setting (tuuli has none). See
    // AttachmentSaver.
    AttachmentSaver {
        id: attachmentSaver
        objectName: "attachmentSaver"
        onSaved: notice.show(attachmentSaver.savedText)
        onError: page.errorMessage = message
    }

    // App store reports saved file. By URL (`Sailfish.WebView`): missing type
    // costs this button. Apps-on checked by tray/row.
    function pickApp() {
        var store = pageStack.push(Qt.resolvedUrl("WebxdcStorePage.qml"),
                                   { accountId: page.accountId })
        if (store) {
            store.picked.connect(page.attach)
        }
    }

    // Webxdc app. Pushed by URL (`Sailfish.WebView`) with message, not file:
    // core knows app files by message.
    function openApp(messageId) {
        pageStack.push(Qt.resolvedUrl("WebxdcPage.qml"), {
            accountId: page.accountId,
            messageId: messageId
        })
    }

    function sendCurrentText() {
        // Model and button refuse too; third check: double send can't be undone.
        if (messages.sending) {
            return
        }
        // Recording under way: that is sent.
        if (voiceBar.recording) {
            voiceBar.send()
            return
        }
        // Shrinking video: button and keyboard wait.
        if (page.videoPreparing) {
            return
        }
        // Editing: change to core, field restored after; no reply or file.
        if (page.editing) {
            var changed = textField.text.trim()
            if (changed.length > 0) {
                page.errorMessage = ""
                messages.edit_message(page.editingMessageId, changed)
            }
            return
        }
        // Bars cleared in `onSent`, else lost on failed send. Blank: no message.
        var text = textField.text.trim()
        // Reader sends it, with whatever they typed.
        page.sendWhenReady = false
        if (page.attachmentPath.length > 0) {
            page.errorMessage = ""
            messages.send_file(text, page.attachmentPath)
        } else if (text.length > 0) {
            page.errorMessage = ""
            messages.send(text)
        }
    }
}
