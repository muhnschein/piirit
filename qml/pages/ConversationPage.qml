import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Format.js" as Format
import "../js/Emoji.js" as Emoji
import Piirit 1.0

/*
 * One conversation. Messages come from this page's own ChatMessages, so a
 * second open conversation cannot reset this one's model.
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
        // File on attachment bar, for model to weigh against core's
        // recommended size. Set aside while editing (edits have no file).
        pending_file: page.editing ? "" : page.attachmentPath
        onError: {
            page.errorMessage = message
            // Unsent voice message: discard, else lingers on phone for nothing.
            page.dropVoice()
            // Refused file stays on bar; not retried on its own.
            if (page.sendingOnItsOwn) {
                page.sendingOnItsOwn = false
                page.sendWhenReady = false
            }
        }
        // Sending means read: go to message just sent, not missed.
        onSent: {
            // File sent alone (`sendWhenReady`) keeps field, reply, draft.
            var onItsOwn = page.sendingOnItsOwn
            page.sendingOnItsOwn = false
            if (!onItsOwn) {
                textField.text = ""
                // Now: chat list would show sent message as still-held draft.
                page.storeDraft()
                page.replyBody = ""
                page.replyAuthor = ""
            }
            // Capture is copied by core; library file stays where it was.
            captures.discard(page.attachmentPath)
            page.attachmentPath = ""
            page.dropVoice()
            page.sendWhenReady = false
            listView.jumpToNewest()
        }
        // Core size limit, video shrink target, rows loaded: `sendWhenReady`
        // waits on these.
        onAttachment_limit_changed: sharedSend.restart()
        onPreparing_changed: sharedSend.restart()
        onLoaded_changed: sharedSend.restart()
        // Edit is not a send: field restored after.
        onEdited: page.finishEdit()
        onArrived: listView.noteArrivals(count)
        // Unsent text, once core answers.
        //
        // `draftApplied` set only when text lands in field. Chat handover
        // clears draft and signals it; taking that as the answer marked field
        // filled before core replied, so draft never came back.
        onDraft_changed: {
            if (!page.draftApplied && messages.draft.length > 0
                    && textField.text.length === 0) {
                textField.text = messages.draft
                page.draftApplied = true
            }
        }
    }

    // Handed over as page is built: prefetched chat arrives with messages.
    //
    // Not deferred to Active: chat opens on one page of fifty, so no long
    // history built on Qt thread to freeze transition; prefetch usually built
    // rows already (no core round trip).
    //
    // In `Component.onCompleted`, not a binding: must run after
    // `reading_history` bound, else model sees default `false` as reader
    // looking and marks chat read before page shows.
    /// Share that opened this chat: file for bar, text for field. See
    /// qml/share/ShareTarget.qml.
    /// Camera quick action does the same (ChatListPage.capture).
    property string sharedFile: ""
    property string sharedText: ""
    /// Shared file sent without send button (camera quick action set to send at
    /// once). Read at creation.
    property bool sendShared: false

    Component.onCompleted: {
        messages.chat_id = page.chatId
        // Share fills chat with shared content for caption and send. Before
        // draft arrives, which only fills an empty field: existing draft kept.
        if (page.sharedFile.length > 0) {
            page.attach(page.sharedFile)
            page.sendWhenReady = page.sendShared
        }
        if (page.sharedText.length > 0) {
            textField.text = page.sharedText
        }
    }

    /// Shared file sent once core's size limit is known and any video shrink
    /// done (else video goes full size), and rows are in: a send during the
    /// read lands on rows the read replaces, message missing until reopen.
    /// Cleared once sent, refused, or taken over (other file, shrink stopped,
    /// reply/edit begun, reader sends).
    property bool sendWhenReady: false
    /// Send under way is that one, not reader's.
    property bool sendingOnItsOwn: false

    // One event turn after model moved: model mid-change can't take send.
    //
    // File alone: field text is draft, not caption. Reply or edit begun
    // meanwhile: file waits.
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

    // List place held while page is away, restored to the pixel.
    onStatusChanged: {
        if (page.status === PageStatus.Deactivating) {
            listView.rememberPlace()
            // Written now: leaving is when debounce hasn't fired.
            page.storeDraft()
            // Close tray: left open under picker, it reopens on return over
            // picked file.
            attachButton.close()
            // Deletes asked for go now, same reason as draft.
            listView.flushDeletes()
        } else if (page.status === PageStatus.Active) {
            listView.restorePlace()
            // Asked for while page away (media page): lands on top of restored
            // place, not under it.
            page.revealIfAsked()
            page.attachInfo()
        }
    }

    /// Go to one message: search result, or media page's ask on return
    /// (ChatMediaPage.showInChat).
    /// Acted on now if on screen, else kept: return restores place, and
    /// message must land over that.
    function showMessage(messageId) {
        page.findMessageId = messageId
        if (page.status === PageStatus.Active) {
            page.revealIfAsked()
        }
    }

    /// Ask model where message is, once rows are in. Answer in `onRevealed`.
    function revealIfAsked() {
        if (page.findMessageId !== 0 && messages.loaded) {
            messages.reveal(page.findMessageId)
        }
    }

    /// Chat's own draft in field yet. Core answers late; typing reader must not
    /// be overwritten.
    property bool draftApplied: false

    function storeDraft() {
        draftDebounce.stop()
        // Not while field holds sent message: core's copy is reader's unsent
        // words.
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

    /// Reader's own message being edited; 0 for none. Edit is a field mode:
    /// field holds its text, bar says so, tray out of reach, send means "send
    /// the change" (as deltachat-android/ios).
    property int editingMessageId: 0
    /// Text as sent, for bar.
    property string editingBody: ""
    /// Field content before edit (unsent draft), restored on send or cancel.
    /// Reference clients discard it; deliberate.
    property string stashedDraft: ""
    readonly property bool editing: page.editingMessageId !== 0

    function beginEdit(messageId, body) {
        // Second Edit keeps first stash: field now holds first message's text.
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

    /// Edit reached core: as cancel, for the field.
    function finishEdit() {
        page.cancelEdit()
    }

    // Search result lands here: waits for model, not page. Found message may be
    // unloaded: `reveal` steps back until it is.
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
                // Held, not jumped to: page still arriving, rows still being
                // measured; one jump lands wherever measuring got to.
                listView.holdAt(row)
                foundFlash.restart()
            }
            // Once only: a later reload must not pull reader back.
            page.findMessageId = 0
        }
        // Fill takes a page at a time; asks again on finish until nothing left.
        onHydrating_changed: if (!messages.hydrating) listView.askForRows()
    }

    // Flash: "this one", then out of the way.
    Timer {
        id: foundFlash
        interval: 4000
        onTriggered: listView.foundMessageId = 0
    }

    // Message counts as seen only if app in front, page on screen, view at
    // newest, no drag. Any false: read receipt would lie.
    // `following` not `stickToBottom`: latter is stale during drag.
    readonly property bool readerIsLooking:
        Qt.application.state === Qt.ApplicationActive
        && page.status === PageStatus.Active
        && listView.following

    property string errorMessage: ""
    // Three states: core going away is handled; no restart banner.
    readonly property string coreStatusMessage:
        core.status === "reconnecting"
        ? qsTr("Lost the connection to the Delta Chat core. Reconnecting…")
        : core.status === "stopped"
          ? qsTr("Lost the connection to the Delta Chat core. Restart Piirit.")
          : ""

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core
        // Model ignores other accounts' and chats' events.
        onCore_event: messages.handle_event(context_id, kind, payload_json)
        // Model made before core is up has nothing to load.
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

    // File next send carries; one per message (ChatMessages.send_file).
    property string attachmentPath: ""
    readonly property string attachmentName: page.displayName(page.attachmentPath)

    // Decoded as shim decodes sent path: picker URLs escape spaces; bar names
    // file as reader does. Undecodable escape means literal '%' in name.
    function displayName(path) {
        var name = path.substring(path.lastIndexOf('/') + 1)
        try {
            return decodeURIComponent(name)
        } catch (error) {
            return name
        }
    }

    // Pickers report back, not send: picked file stays cancellable, caption
    // can follow.
    function attach(path) {
        if (path && path.length > 0) {
            // Capture replaced before send goes nowhere.
            captures.discard(page.attachmentPath)
            page.attachmentPath = path
            // Only shared file goes on its own.
            page.sendWhenReady = false
        }
    }

    // Where photos taken here and voice messages wait to send; this page
    // discards them after.
    Captures {
        id: captures
        objectName: "captures"
    }

    // Cancel discards capture; cancel of picked file leaves it.
    function dropAttachment() {
        captures.discard(page.attachmentPath)
        page.attachmentPath = ""
    }

    /// Voice message on its way to core, until answered. Goes recorder -> send,
    /// never via bar, so kept here to discard after.
    property string pendingVoice: ""

    function dropVoice() {
        captures.discard(page.pendingVoice)
        page.pendingVoice = ""
    }

    // Capture left on bar at close, or voice message core never answered, goes
    // nowhere now: nothing else knows of it. Unless a send is in flight (may
    // still read it): sweep (capture.rs) handles that. Recoded video is the
    // model's to put away.
    Component.onDestruction: {
        if (!messages.sending) {
            page.dropAttachment()
            page.dropVoice()
        }
    }

    // Pushed by URL like ChatPickerPage: only picker pages name
    // `Sailfish.Pickers`, only capture page and scanner name Camera, so a
    // missing type costs one button, not the chat.
    function pickWith(pageName) {
        var picker = pageStack.push(Qt.resolvedUrl(pageName))
        // Null when page failed to load: connecting would throw.
        if (picker) {
            picker.picked.connect(page.attach)
        }
    }

    // Outside list: header scrolls off top in long chats. Not PageHeader: name
    // is other end's to choose, and that header can't show it as plain text.
    ConversationHeader {
        id: conversationHeader
        objectName: "conversationHeader"
        title: page.chatName
        // Group member count under name. None for one-to-one ("2" for one
        // person), or before chat shape arrives ("0 members" flicker).
        //: Under a group's name, over its messages. %n is how many
        //: people are in the group.
        subtitle: messages.is_group && messages.member_count > 0
                  ? qsTr("%n member(s)", "", messages.member_count)
                  : ""
        // White once info page attached, as PageHeader is when navigable.
        interactive: page.canNavigateForward
        // Name goes the same way the swipe does.
        onClicked: page.openInfo()
    }

    /// Call this chat, or go back to the call there is. One call at a time (as
    /// reference clients): asking again brings first one to front.
    function placeCall(video) {
        if (typeof appWindow === "undefined") {
            return
        }
        if (!appWindow.placeCall(page.accountId, page.chatId, video === true)) {
            appWindow.showCall()
        }
    }

    /// Call row tapped: ringing call answered here (may have rung while nothing
    /// listened), as reference clients; any other called back as made (video
    /// with camera on).
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

    // Group or contact info, to the right; which depends on chat kind, so waits
    // for load. Attached, not pushed: indicator and swipe reach it. Reattached
    // each return: a pushed page takes it along.
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
        // Name from list until core says; renames elsewhere reach header via
        // model.
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
        // Model count: changes when row arrives, not when view shows it.
        messageCount: messages.count
        // Model fills rows on screen; this says which those are.
        onHydrateRequested: messages.hydrate(first, last)
        showSender: messages.is_group
        // Reader's last-read place, asked of core on open.
        unreadFrom: messages.unread_from
        markdownMode: Settings.markdownMode
        // Off until asked: `.xdc` is a file until then. `=== true`: dconf
        // undefined before read.
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
        // On/off is model's call: it knows what reader sent; core takes whole
        // list either way.
        onReactionRequested: messages.react(messageId, emoji)
        // Other emoji opens a page; id captured now (row may be gone).
        onReactionPickerRequested: {
            var reactingTo = messageId
            var mine = current
            var picker = pageStack.push(
                Qt.resolvedUrl("ReactionPickerPage.qml"),
                { current: mine, recent: Settings.recentReactions })
            if (picker) {
                picker.picked.connect(function(emoji) {
                    messages.react(reactingTo, emoji)
                    // Removing own reaction is not offered again.
                    if (emoji !== mine) {
                        Settings.recentReactions = Emoji.remember(
                            Settings.recentReactions, emoji)
                    }
                })
            }
        }
        // Delete kind picked on menu's page, wait over: from this account or
        // everybody's.
        onDeleteRequested: {
            if (forEveryone) {
                messages.delete_message_for_all(messageId)
            } else {
                messages.delete_message(messageId)
            }
        }
        // Menu asks for delete; dialog pushed by page. Nothing deleted until
        // dialog accepted and wait done: back edge deletes nothing.
        onDeleteChoiceRequested: {
            // Taken now, not in callback: row may be gone (as Forward hoists
            // its id).
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
            // Picker reports back; message id captured here.
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

    // Between list and field: translucent, messages would show through.
    Banner {
        id: banner
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
        // Core state outranks earlier failure: read status, don't wait for
        // change.
        text: page.coreStatusMessage.length > 0
              ? page.coreStatusMessage : page.errorMessage
        // Reconnect ends when status says; stopped never does.
        timeout: page.coreStatusMessage.length > 0 ? 0 : 8
        onDismissed: page.errorMessage = ""
    }

    // Reply or edit bar, one for both: can't want both (edit isn't a send);
    // reply returns once edit done.
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

    // Directly above field: reads as the message about to be sent.
    AttachmentBar {
        id: attachmentBar
        objectName: "attachmentBar"
        anchors {
            left: parent.left
            right: parent.right
            bottom: largeFileBar.top
        }
        // Set aside while editing: edit has no file. File still here after.
        filePath: page.editing ? "" : page.attachmentPath
        fileName: page.attachmentName
        // Picked video shrinks as picked; its button stops shrink, keeps file;
        // tap again drops file.
        preparing: messages.preparing
        progress: messages.preparing_progress
        // How much smaller it went or will go.
        originalBytes: page.editing ? 0 : messages.original_bytes
        bytes: messages.attachment_bytes
        onCancelled: page.dropAttachment()
        // Stopped video goes as is, even if set to send at once.
        onStopped: {
            page.sendWhenReady = false
            messages.cancel_preparing()
        }
    }

    // File over core's recommended size (some relays refuse): warning, not
    // refusal, send stays on (one size for all relays, many take more, issue
    // #101). Said before upload, not after failed send. Never a picture (core
    // shrinks those); videos weighed at shrink size. See
    // rust/piirit-shim/src/media.rs and video.rs.
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

    // Says what happened where page has no state (e.g. clipboard).
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

    /// Text and file: what send sends. Editing: text only.
    readonly property bool hasSomethingToSend:
        textField.text.trim().length > 0
        || (!page.editing && page.attachmentPath.length > 0)

    // Tap outside tray closes it, as Silica menus do. Declared here: over list
    // and bars above, under input row declared after.
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
            // Mirrors field's own left inset; else send button hugs edge.
            rightMargin: Theme.horizontalPageMargin
            bottom: parent.bottom
            // Off screen edge: TextArea has no room under text; recording strip
            // less still.
            bottomMargin: Theme.paddingLarge
        }
        spacing: Theme.paddingSmall

        // Tall enough for field and lifted buttons: Row ignores child anchors.
        height: Math.max(textField.visible ? textField.height : 0,
                         voiceBar.visible ? voiceBar.height : 0,
                         sendButton.height + inputRow.buttonLift)

        /// How much higher than the field the buttons sit. None: Silica field
        /// keeps descender room, so level button already sits above underline;
        /// padding lift made them float.
        readonly property real buttonLift: 0

        // The recording, where the field was, while there is one.
        VoiceBar {
            id: voiceBar
            objectName: "voiceBar"
            width: parent.width - sendButton.width
            anchors.verticalCenter: sendButton.verticalCenter
            // Recording stops at core's recommended size (as large-file warning;
            // over an hour of speech), at media-quality bit rate.
            limitBytes: messages.attachment_limit
            mediaQuality: Settings.mediaQuality === 1 ? 1 : 0
            onRecorded: {
                page.errorMessage = ""
                page.pendingVoice = path
                messages.send_voice(path)
            }
            onFailed: page.errorMessage = message
        }

        // Both step aside while recording (Row lays out visible only). Field is
        // multi-line: return inserts newline; send is the button.
        TextArea {
            id: textField
            objectName: "messageField"
            visible: !voiceBar.recording
            // Only visible buttons: Row lays out visible; tray steps aside on
            // edit.
            width: parent.width - (attachButton.visible ? attachButton.width : 0)
                   - sendButton.width
            // Against row bottom: Row lays from top, so field would rise with
            // lifted buttons.
            anchors.bottom: parent.bottom
            //: Message field placeholder. Also the prompt for the caption
            //: on a message that is carrying a file.
            placeholderText: page.attachmentPath.length > 0
                             ? qsTr("Caption") : qsTr("Message")
            // Silica label says same as placeholder.
            labelVisible: false
            // Grows to a third of screen, then scrolls.
            height: Math.min(implicitHeight, page.height / 3)
            // Kept in core across restarts; chat list shows which chats hold
            // one.
            onTextChanged: draftDebounce.restart()
            // Typing is not a tap on tray.
            onActiveFocusChanged: if (activeFocus) attachButton.close()
        }

        // Buttons at row bottom: beside draft's last line, where writing
        // happens.
        AttachButton {
            id: attachButton
            objectName: "attachButton"
            // Not while editing: edit has nothing to attach.
            visible: !voiceBar.recording && !page.editing
            anchors.bottom: parent.bottom
            anchors.bottomMargin: inputRow.buttonLift
            voiceAvailable: voiceBar.available
            appsAvailable: Settings.webxdcEnabled === true
            onCameraRequested: page.pickWith("CapturePage.qml")
            onLibraryRequested: page.pickWith("AttachLibraryPage.qml")
            onVoiceRequested: voiceBar.start()
            // `.xdc` sent as any file; core sends it as an app.
            onAppRequested: page.pickApp()
        }

        IconButton {
            id: sendButton
            objectName: "sendButton"
            anchors.bottom: parent.bottom
            anchors.bottomMargin: inputRow.buttonLift
            // Hidden, not greyed, while sending: keeps row shape. While
            // editing: tick keeps change.
            icon.source: messages.sending ? ""
                         : page.editing ? "image://theme/icon-m-accept"
                                        : "image://theme/icon-m-send"
            // File alone is a message; empty or blank is not. Recording: send
            // stops and sends it. No double send (large video copy into core
            // lets a second tap through). Shrinking video: not until ready and
            // reader sends, unless `sendWhenReady`.
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

    // Pictures and videos shown here: system handover failed to play them.
    // Rest handed on: a "cannot show" page is worse. No file page: copy is on
    // row menu.
    function openAttachment(fileUrl, fileName, viewType, previewWidth) {
        if (viewType === "Image" || viewType === "Gif"
                || viewType === "Sticker") {
            pageStack.push(Qt.resolvedUrl("PicturePage.qml"), {
                fileUrl: fileUrl,
                fileName: fileName,
                viewType: viewType,
                // How wide the row drew it, when it came from a row.
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

    // Copy to Piirit's Downloads folder. No setting (tuuli has none). See
    // AttachmentSaver.
    AttachmentSaver {
        id: attachmentSaver
        objectName: "attachmentSaver"
        onSaved: notice.show(attachmentSaver.savedText)
        onError: page.errorMessage = message
    }

    // Apps from store, which reports file it put on phone. Pushed by URL:
    // names `Sailfish.WebView`, so missing type costs this button only.
    // Apps-on not checked here or in `openApp`: tray and row already bound to
    // setting.
    function pickApp() {
        var store = pageStack.push(Qt.resolvedUrl("WebxdcStorePage.qml"),
                                   { accountId: page.accountId })
        if (store) {
            store.picked.connect(page.attach)
        }
    }

    // Webxdc app, run here. Pushed by URL (`Sailfish.WebView`) with message,
    // not file: core knows app files by message.
    function openApp(messageId) {
        pageStack.push(Qt.resolvedUrl("WebxdcPage.qml"), {
            accountId: page.accountId,
            messageId: messageId
        })
    }

    function sendCurrentText() {
        // Model refuses and button is off too; cheap third check: a double
        // send can't be taken back.
        if (messages.sending) {
            return
        }
        // A recording under way is what the button sends.
        if (voiceBar.recording) {
            voiceBar.send()
            return
        }
        // Shrinking video: button and keyboard wait.
        if (page.videoPreparing) {
            return
        }
        // Editing: change goes to core, field restored after. Reply and file
        // aren't part of it.
        if (page.editing) {
            var changed = textField.text.trim()
            if (changed.length > 0) {
                page.errorMessage = ""
                messages.edit_message(page.editingMessageId, changed)
            }
            return
        }
        // Bars cleared in `onSent` with model's copy: else lost on failed send.
        // Whitespace-only is no message.
        var text = textField.text.trim()
        // The reader is sending it themselves, with whatever they typed.
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
