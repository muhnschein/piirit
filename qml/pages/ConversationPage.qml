import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Format.js" as Format
import Piirit 1.0

/*
 * One conversation. The messages come from a ChatMessages instance owned by
 * this page, so a second open conversation cannot reset this one's model.
 */
Page {
    id: page

    property int accountId
    property int chatId
    property string chatName
    /// A message a search found in this chat, to open at rather than at
    /// the newest. 0 opens the chat normally.
    property int findMessageId: 0

    ChatMessages {
        id: messages
        objectName: "messages"
        account_id: page.accountId
        // What the reader can actually see decides what counts as read.
        reading_history: !page.readerIsLooking
        // The reader's setting, from the settings page: known tracking
        // parameters come out of links on the way out.
        clean_links: Settings.cleanLinks === true
        // What is waiting on the attachment bar, so the model can weigh
        // it against what this profile's relay takes. Put aside with the
        // bar itself while a message is being edited, since an edit
        // carries no file.
        pending_file: page.editing ? "" : page.attachmentPath
        onError: {
            page.errorMessage = message
            // A voice message that could not be sent is still on the
            // phone for nothing.
            page.dropVoice()
        }
        // Sending is its own answer to "have I read this": go to the
        // message just sent rather than counting it as one that was missed.
        onSent: {
            textField.text = ""
            // Now, not a second from now: the chat list would otherwise
            // show the message as a draft it is still holding, next to the
            // same message as the one just sent.
            page.storeDraft()
            page.replyBody = ""
            page.replyAuthor = ""
            // A capture -- a picture taken here, a voice message -- has
            // been copied by the core and is not needed any more. A
            // file picked from a library is not a capture, and is left
            // where it was.
            captures.discard(page.attachmentPath)
            page.attachmentPath = ""
            page.dropVoice()
            listView.jumpToNewest()
        }
        // An edit is not a send: the row shows the new text, and the
        // field goes back to whatever was in it before the edit began.
        onEdited: page.finishEdit()
        onArrived: listView.noteArrivals(count)
        // The chat's unsent text, once the core has answered with it.
        //
        // `draftApplied` is set only when something is actually put in the
        // field. Handing the chat over clears the draft and says so, and
        // treating that as the answer would mark the field filled before
        // the core had replied -- which is how this first went in and why
        // nothing came back.
        onDraft_changed: {
            if (!page.draftApplied && messages.draft.length > 0
                    && textField.text.length === 0) {
                textField.text = messages.draft
                page.draftApplied = true
            }
        }
    }

    // The chat is handed over as the page is built, so a prefetched one is
    // already in the model before the transition starts and the page comes
    // in with its messages rather than filling in behind itself.
    //
    // Not deferred to PageStatus.Active: a chat opens on one page of fifty
    // rather than whole, so there is no long history to build on the Qt
    // thread and freeze the transition with. The prefetch has usually built
    // those rows already, leaving the handover a move with no core round
    // trip in it at all.
    //
    // In `Component.onCompleted` rather than a binding on the declaration
    // above, because the order matters: this must run after
    // `reading_history` has been bound, or the model would see the default
    // `false`, take it for a reader looking at the screen, and mark the
    // chat read before the page is even on it.
    /// What a share handed this chat, when the page was opened from one:
    /// a file to put on the attachment bar, text to put in the field.
    /// See qml/share/ShareTarget.qml.
    property string sharedFile: ""
    property string sharedText: ""

    Component.onCompleted: {
        messages.chat_id = page.chatId
        // A share opens the chat with what was shared already in it,
        // for the reader to add a caption or a word to and send. Before
        // the draft arrives, which only fills a field that is empty --
        // so a chat holding a draft keeps it under what was shared
        // rather than over it.
        if (page.sharedFile.length > 0) {
            page.attach(page.sharedFile)
        }
        if (page.sharedText.length > 0) {
            textField.text = page.sharedText
        }
    }

    // The list's place is remembered as a page goes over this one, held
    // while it is away, and put back -- to the pixel, so that a view
    // nothing has moved does not move -- when this one is active again.
    // Opening a picture full screen and coming back is the way most
    // readers meet that.
    onStatusChanged: {
        if (page.status === PageStatus.Deactivating) {
            listView.rememberPlace()
            // Written now rather than a second from now: leaving the chat
            // is exactly when the debounce below has not fired yet, and
            // that was the whole complaint.
            page.storeDraft()
            // A tray left open under a picker is open again on the way
            // back, over the file just picked.
            attachButton.close()
            // And messages the reader asked to delete go now, for the
            // same reason the draft is written now: a reader who asked
            // for one to go and then left the chat asked for it to go.
            listView.flushDeletes()
        } else if (page.status === PageStatus.Active) {
            listView.restorePlace()
            // A message asked for while the page was away -- from a
            // media page over the contact's, on its way back here --
            // lands on top of the place just put back, not under it.
            page.revealIfAsked()
            page.attachInfo()
        }
    }

    /// Go to one message: what a search result asks for as the chat
    /// opens, and what a media page over this one asks for on the way
    /// back (ChatMediaPage.showInChat). Acted on now if this is the page
    /// on screen; kept until it is otherwise, since coming back puts
    /// the view where the reader left it and the message has to land
    /// over that.
    function showMessage(messageId) {
        page.findMessageId = messageId
        if (page.status === PageStatus.Active) {
            page.revealIfAsked()
        }
    }

    /// Ask the model where the message that was asked for is, once
    /// there is one and the rows are in. Where it answers is
    /// `onRevealed` below.
    function revealIfAsked() {
        if (page.findMessageId !== 0 && messages.loaded) {
            messages.reveal(page.findMessageId)
        }
    }

    /// Whether the chat's own draft has been put in the field yet.
    ///
    /// The answer comes back from the core a moment after the page opens,
    /// and a reader who started typing in that moment must not have it
    /// written over them.
    property bool draftApplied: false

    function storeDraft() {
        draftDebounce.stop()
        // Not while the field holds a message already sent: what the
        // core is keeping is the reader's own unsent words, put aside
        // for the edit and put back after it.
        if (page.editing) {
            return
        }
        messages.save_draft(textField.text)
    }

    // Not on every keystroke: that is one call to the core per character.
    Timer {
        id: draftDebounce
        interval: 1000
        onTriggered: page.storeDraft()
    }

    /// The message of the reader's own whose text is in the field to be
    /// changed, 0 for none. Editing is a mode of the field: while it is
    /// on, the field holds that message's text, the bar above it says
    /// so, the attachment tray is out of reach, and send means "send the
    /// change" -- the shape deltachat-android and deltachat-ios give it.
    property int editingMessageId: 0
    /// The text as it was sent, for the bar to show.
    property string editingBody: ""
    /// What the field held before the edit began: the reader's unsent
    /// draft, put back when the edit is sent or cancelled. The reference
    /// clients throw it away, which is a loss with no reason behind it.
    property string stashedDraft: ""
    readonly property bool editing: page.editingMessageId !== 0

    function beginEdit(messageId, body) {
        // A second Edit while one is under way keeps the first stash:
        // what is in the field now is the first message's text, not
        // anything the reader wants back.
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

    /// The edit reached the core: the same as cancelling, as far as the
    /// field is concerned.
    function finishEdit() {
        page.cancelEdit()
    }

    // Where a search result lands. The row cannot be looked up until the
    // fetch has finished, so this waits for the model to say so rather
    // than for the page: the two are not the same moment. And a chat opens
    // on its newest page, so the message a search found may not be loaded
    // at all -- `reveal` steps back until it is and then says where it
    // went, which is why this is two handlers rather than one lookup.
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
                // Held rather than jumped to: the page is still arriving
                // and its rows still being measured, and one jump lands
                // the reader wherever the measuring has got to.
                listView.holdAt(row)
                foundFlash.restart()
            }
            // Once is enough, found or not: a later reload must not drag
            // the reader back off whatever they have scrolled to since.
            page.findMessageId = 0
        }
        // A fill takes at most a page at a time, and a screenful of a chat
        // with big rows in it can want more than that. Asking again when
        // one finishes is what covers the rest. It stops on its own: the
        // ask is dropped when there is nothing left to fill, so nothing
        // comes back to prompt another.
        onHydrating_changed: if (!messages.hydrating) listView.askForRows()
    }

    // The flash says "this one" and then gets out of the way.
    Timer {
        id: foundFlash
        interval: 4000
        onTriggered: listView.foundMessageId = 0
    }

    // Everything that has to hold for an arriving message to count as seen:
    // the app is in front, this page is the one on screen, and the view is
    // at the newest message and not mid-gesture. `following` rather than
    // `stickToBottom` because the latter is only recomputed when a drag
    // ends, so it still reads true throughout a drag away from the bottom.
    // Any one of these false means a read receipt would be a lie.
    readonly property bool readerIsLooking:
        Qt.application.state === Qt.ApplicationActive
        && page.status === PageStatus.Active
        && listView.following

    property string errorMessage: ""
    // Three states, not two: the core going away is now something the app
    // does something about, and a banner that says "restart Piirit"
    // while Piirit is already fixing it is worse than none.
    readonly property string coreStatusMessage:
        core.status === "reconnecting"
        ? qsTr("Lost the connection to the Delta Chat core. Reconnecting…")
        : core.status === "stopped"
          ? qsTr("Lost the connection to the Delta Chat core. Restart Piirit.")
          : ""

    // Qt 5.6 handler syntax; see WelcomePage.qml.
    Connections {
        target: core
        // The model ignores events for other accounts and chats itself.
        onCore_event: messages.handle_event(context_id, kind, payload_json)
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                messages.reload()
            }
        }
        onCore_error: page.errorMessage = message
    }

    // What the reader picked Reply on, for the bar above the field. The id
    // itself lives on the model, which is what the send reads.
    property string replyBody: ""
    property string replyAuthor: ""

    function cancelReply() {
        messages.quoted_message_id = 0
        page.replyBody = ""
        page.replyAuthor = ""
    }

    // The file the next send will carry, empty for none. One per message,
    // which is the core's own shape -- see ChatMessages.send_file.
    property string attachmentPath: ""
    readonly property string attachmentName: page.displayName(page.attachmentPath)

    // Decoded for the same reason the shim decodes the path it sends: a
    // picker that hands back a URL escapes the spaces, and the bar should
    // name the file the way the reader does. An escape that does not decode
    // is a name with a literal '%' in it.
    function displayName(path) {
        var name = path.substring(path.lastIndexOf('/') + 1)
        try {
            return decodeURIComponent(name)
        } catch (error) {
            return name
        }
    }

    // Both pickers report back here rather than sending, so a picked file
    // can still be cancelled, and so a caption can be typed after choosing.
    function attach(path) {
        if (path && path.length > 0) {
            // A capture that is replaced before it is sent is not going
            // anywhere.
            captures.discard(page.attachmentPath)
            page.attachmentPath = path
        }
    }

    // Where a picture taken here, or a voice message, waits to be sent;
    // this page is what takes it back afterwards.
    Captures {
        id: captures
        objectName: "captures"
    }

    // Cancelling a capture throws the file away; cancelling a picked
    // file leaves it where it was.
    function dropAttachment() {
        captures.discard(page.attachmentPath)
        page.attachmentPath = ""
    }

    /// The voice message on its way to the core, until the core has
    /// answered: it goes straight from the recorder to the send, never
    /// through the attachment bar, so it is remembered here to be
    /// discarded afterwards.
    property string pendingVoice: ""

    function dropVoice() {
        captures.discard(page.pendingVoice)
        page.pendingVoice = ""
    }

    // Pushed by URL and connected to, the way forwarding pushes
    // ChatPickerPage: the picker pages are the only files that name a
    // `Sailfish.Pickers` type, and the capture page and the scanner the
    // only ones that name a Camera, so a type that is not there costs one
    // button rather than the whole conversation.
    function pickWith(pageName) {
        var picker = pageStack.push(Qt.resolvedUrl(pageName))
        // Null when the page could not be loaded, which is the case the
        // comment above is about; connecting to it would throw.
        if (picker) {
            picker.picked.connect(page.attach)
        }
    }

    // Outside the list rather than its `header`, so it stays put: a
    // header scrolls with the content, and in a long conversation the
    // name of whoever you are talking to disappears off the top. Not
    // Silica's PageHeader: the name is the other end's to choose, and
    // that header cannot be told to show it as plain text.
    ConversationHeader {
        id: conversationHeader
        objectName: "conversationHeader"
        title: page.chatName
        // Under a group's name, where PageHeader puts a description:
        // how many people are in it. Nothing under a one-to-one chat's
        // name, where the count would say "2" about a conversation with
        // one other person, and nothing until the chat's shape has
        // arrived -- a "0 members" that turns into seven reads as the
        // group having been empty a moment ago.
        //: Under a group's name, over its messages. %n is how many
        //: people are in the group.
        subtitle: messages.is_group && messages.member_count > 0
                  ? qsTr("%n member(s)", "", messages.member_count)
                  : ""
        // White once the info page is attached, as a PageHeader is on a
        // page that can navigate forward.
        interactive: page.canNavigateForward
        // The name goes the same way the swipe does.
        onClicked: page.openInfo()
    }

    // What this chat is -- the group, or the contact -- sits to the right,
    // attached rather than pushed: the page indicator says it is there,
    // and a swipe reaches it. Which page depends on the kind of chat, so
    // it waits for the load; and it is attached again on every return,
    // since a page pushed over this one takes the attached one with it.
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
        // The name the list handed over, until the core says otherwise:
        // a rename on the page beside this one, or on another device,
        // reaches the header through the model rather than through
        // whichever page did it.
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
        // The model's own count, which changes when a row arrives rather
        // than when the view gets round to showing it.
        messageCount: messages.count
        // The model holds a row for every message and fills in the ones on
        // screen; this is what tells it which those are.
        onHydrateRequested: messages.hydrate(first, last)
        showSender: messages.is_group
        // Where the reader left off, which the model asked the core for
        // when the chat was opened.
        unreadFrom: messages.unread_from
        markdownMode: Settings.markdownMode
        // Off until the reader asks for it, so a `.xdc` in the chat is a
        // file until then. `=== true` because dconf hands back
        // `undefined` before it has read the key.
        appsEnabled: Settings.webxdcEnabled === true
        // What the core would take an edit in: see the model.
        canEdit: messages.can_send && messages.is_encrypted
        placeholderText: qsTr("No messages yet")

        // Reaching the newest message is what marks what is there read.
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
        onDownloadRequested: messages.download_full(messageId)
        // On or off is the model's call: it knows what the reader already
        // sent, and the core takes the whole list either way.
        onReactionRequested: messages.react(messageId, emoji)
        // Which kind of delete was picked on the page the menu led to,
        // and the wait is up: the message goes from this account's
        // devices, or from everybody's.
        onDeleteRequested: {
            if (forEveryone) {
                messages.delete_message_for_all(messageId)
            } else {
                messages.delete_message(messageId)
            }
        }
        // The menu asks for a deletion; which kind is a dialog, and
        // pushing it is the page's business rather than the list's.
        // Nothing is deleted until that dialog is accepted and the wait
        // after it is up, so leaving by the back edge deletes nothing.
        onDeleteChoiceRequested: {
            // Taken now rather than in the callback: choosing takes a
            // page push, and the row may be gone by the time the answer
            // comes back -- the same reason Forward hoists its id.
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
            // The picker reports back rather than acting, so the
            // message id is captured here where it is still valid.
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

    // Only up when the reader has scrolled away from the newest message
    // and something has arrived below them meanwhile: a reader browsing
    // the history knows where the end is.
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

    // Between the list and the field rather than over the list: it is
    // translucent, and the messages behind it showed through.
    Banner {
        id: banner
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
        // The core's own state outranks whatever failed before it, and a
        // page opened after it went away never saw the transition -- so
        // read the status rather than waiting for it to change.
        text: page.coreStatusMessage.length > 0
              ? page.coreStatusMessage : page.errorMessage
        // Neither clears itself: reconnecting ends when the status says so,
        // and stopped does not end at all.
        timeout: page.coreStatusMessage.length > 0 ? 0 : 8
        onDismissed: page.errorMessage = ""
    }

    // What the next send replies to, and a way out of replying -- or,
    // while a message is being edited, which one, and a way out of that.
    // One bar for both: the two cannot be wanted at once, since an edit
    // is not a send, and the reply comes back into view when the edit is
    // done.
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

    // Directly above the field, below the reply bar: reading downwards,
    // the two bars are the message about to be sent, in the order its
    // parts appear in it.
    AttachmentBar {
        id: attachmentBar
        objectName: "attachmentBar"
        anchors {
            left: parent.left
            right: parent.right
            bottom: tooBigBar.top
        }
        // Put aside with the draft while a message is edited: an edit
        // carries no file, and a bar saying one is about to be sent would
        // be saying something untrue. The file is still here afterwards.
        filePath: page.editing ? "" : page.attachmentPath
        fileName: page.attachmentName
        onCancelled: page.dropAttachment()
    }

    // Under the file it is about: this one the relay will not take, so
    // there is nothing to do but pick something smaller. Said here rather
    // than after a send that failed, and the send button is off while it
    // stands -- a picture is never this, since the core shrinks those on
    // the way out. The limit is the core's own recommendation for the
    // profile's relay; see rust/piirit-shim/src/media.rs.
    Banner {
        id: tooBigBar
        objectName: "tooBigBar"
        labelObjectName: "tooBigLabel"
        tone: "error"
        // Not transient: it is true for as long as the file is on the
        // bar.
        timeout: 0
        text: messages.attachment_too_big
              //: Shown above the message field when the attached file is
              //: bigger than will be sent. %1 is the file's size and %2
              //: the largest that goes, each such as "24 MB".
              ? qsTr("%1 is too large to send. Attachments can be up to %2.")
                .arg(Format.readableSize(messages.attachment_bytes))
                .arg(Format.readableSize(messages.attachment_limit))
              : ""
        anchors {
            left: parent.left
            right: parent.right
            bottom: inputRow.top
        }
    }

    // Says what just happened where the page has no state for it, such as
    // a message going to the clipboard.
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

    /// The text and the file: what send has to send. While a message is
    /// being edited the file is put aside, and only the text counts.
    ///
    /// Nothing at all while the file on the bar is bigger than the relay
    /// takes, text included: the caption belongs to the file, and sending
    /// it on its own would drop the file without saying so. The way on is
    /// to take the file off the bar.
    readonly property bool hasSomethingToSend:
        !(!page.editing && messages.attachment_too_big)
        && (textField.text.trim().length > 0
            || (!page.editing && page.attachmentPath.length > 0))

    // A tap anywhere but the tray closes the tray: over everything
    // declared above -- the list, the bars -- and under the input row,
    // which is declared after it. Silica's own menus close the same way.
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
            // The field carries its own inset on the left; without the
            // same on this side the send button sits nearer the edge.
            rightMargin: Theme.horizontalPageMargin
            bottom: parent.bottom
            // Off the edge of the screen rather than on it: a TextArea
            // carries no room under its text the way a TextField does,
            // and the recording strip carries less still.
            bottomMargin: Theme.paddingLarge
        }
        spacing: Theme.paddingSmall

        // Tall enough for the field, and never too short for the buttons
        // to sit in with the lift below: a Row sizes itself to its
        // tallest child and takes no account of what a child's anchors
        // ask for, so a button lifted off the bottom of a short row
        // would be drawn above the row and over the bar above it.
        height: Math.max(textField.visible ? textField.height : 0,
                         voiceBar.visible ? voiceBar.height : 0,
                         sendButton.height + inputRow.buttonLift)

        /// How much higher than the field the buttons sit.
        ///
        /// Nothing: a Silica field keeps room under its line for what
        /// hangs below a letter, so a button level with the field's
        /// bottom edge already sits a little above its underline. Lifted
        /// by a padding on top of that, as this was first written, they
        /// read as floating over the row rather than belonging to it.
        readonly property real buttonLift: 0

        // The recording, where the field was, while there is one.
        VoiceBar {
            id: voiceBar
            objectName: "voiceBar"
            width: parent.width - sendButton.width
            anchors.verticalCenter: sendButton.verticalCenter
            // A recording stops at the longest the relay takes -- the
            // limit a file on the attachment bar is held to -- and is
            // made at the bit rate the reader's outgoing media quality
            // says.
            limitBytes: messages.attachment_limit
            mediaQuality: Settings.mediaQuality === 1 ? 1 : 0
            onRecorded: {
                page.errorMessage = ""
                page.pendingVoice = path
                messages.send_voice(path)
            }
            onFailed: page.errorMessage = message
        }

        // Both step aside while a voice message records: a Row lays out
        // only what is visible, so the strip takes their room.
        //
        // A field of one line could not hold a paragraph and could not
        // hold a line break at all: the return key sent the message, so
        // a message written here was one line by construction, however
        // long. This is an area: return puts in a newline, the field
        // grows as the message does, and send is the button -- which is
        // what every other client on this phone does with a message
        // longer than a remark, and the only way both reference clients
        // offer.
        TextArea {
            id: textField
            objectName: "messageField"
            visible: !voiceBar.recording
            // Only the buttons that are there: a Row lays out what is
            // visible, and the tray steps aside while a message is
            // edited.
            width: parent.width - (attachButton.visible ? attachButton.width : 0)
                   - sendButton.width
            // Against the bottom of the row, as the buttons are: a Row
            // lays its children out from the top, so a field left there
            // would rise with the row whenever the row grew for the
            // buttons' lift -- and the lift would come to nothing.
            anchors.bottom: parent.bottom
            //: Message field placeholder. Also the prompt for the caption
            //: on a message that is carrying a file.
            placeholderText: page.attachmentPath.length > 0
                             ? qsTr("Caption") : qsTr("Message")
            // Silica's own label sits above the text and says the same
            // thing the placeholder does.
            labelVisible: false
            // It grows with what is in it, up to a point: past a third
            // of the screen the conversation it is written in would be
            // gone, so the area keeps that height and scrolls inside it.
            height: Math.min(implicitHeight, page.height / 3)
            // Kept in the core, so it is still here after the app has been
            // closed and reopened, and so the chat list can say which
            // chats are holding one.
            onTextChanged: draftDebounce.restart()
            // Typing is not a tap on the tray either.
            onActiveFocusChanged: if (activeFocus) attachButton.close()
        }

        // Both buttons sit against the bottom of the row rather than the
        // top of it, so that a draft grown to several lines leaves them
        // beside its last line -- where the text being written is --
        // rather than beside its first.
        AttachButton {
            id: attachButton
            objectName: "attachButton"
            // Nor while a message is being edited: an edit changes the
            // words and nothing else, so there is nothing to attach to it.
            visible: !voiceBar.recording && !page.editing
            anchors.bottom: parent.bottom
            anchors.bottomMargin: inputRow.buttonLift
            voiceAvailable: voiceBar.available
            appsAvailable: Settings.webxdcEnabled === true
            onCameraRequested: page.pickWith("CapturePage.qml")
            onLibraryRequested: page.pickWith("AttachLibraryPage.qml")
            onVoiceRequested: voiceBar.start()
            // A .xdc goes out as any other file does: the core sees what
            // it is and sends it as an app. Where one comes from is the
            // store's business.
            onAppRequested: page.pickApp()
        }

        IconButton {
            id: sendButton
            objectName: "sendButton"
            anchors.bottom: parent.bottom
            anchors.bottomMargin: inputRow.buttonLift
            // Hidden rather than greyed while a send is in flight: the
            // indicator takes its place, so the row keeps its shape. A
            // tick while a message is being edited, since what the tap
            // does then is keep a change rather than send a message.
            icon.source: messages.sending ? ""
                         : page.editing ? "image://theme/icon-m-accept"
                                        : "image://theme/icon-m-send"
            // A file on its own is a message; an empty field with nothing
            // attached is not, and neither is one holding only spaces. A
            // recording under way is what send stops and sends. And
            // nothing is sendable twice: copying a large video into the
            // core's blob directory takes long enough for a second tap to
            // land, and that sent the whole thing again.
            enabled: !messages.sending
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

    // Which kinds Piirit shows itself, and which it hands on. Handing a
    // picture or a video to the system took the reader out of the app to
    // something that then failed to play it; everything else is still
    // somebody else's file to open, and a page here that could only say
    // "cannot show this" would be worse than the handover.
    //
    // No page of its own for a file: a page for reading belongs to a long
    // message rather than to an attachment. What a file needs and a tap
    // cannot give is a copy, and that is on the row's menu.
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

    // A copy of an attachment, in Piirit's folder in Downloads -- no
    // setting, as tuuli has none. See AttachmentSaver.
    AttachmentSaver {
        id: attachmentSaver
        objectName: "attachmentSaver"
        onSaved: notice.show(attachmentSaver.savedText)
        onError: page.errorMessage = message
    }

    // Where an app comes from: the store, which reports the file it put
    // on the phone the way a picker reports the file that was chosen.
    // Pushed by URL like the pickers, since it names a `Sailfish.WebView`
    // type and should cost this button rather than the conversation.
    //
    // Neither this nor `openApp` checks whether apps are on: what calls
    // them is a tray entry and a row that are bound to the setting, so
    // with apps off there is nothing to tap. A check here would be a
    // second answer to the same question, and dead either way.
    function pickApp() {
        var store = pageStack.push(Qt.resolvedUrl("WebxdcStorePage.qml"),
                                   { accountId: page.accountId })
        if (store) {
            store.picked.connect(page.attach)
        }
    }

    // A webxdc app, run here. Pushed by URL and given the message rather
    // than the file: the app's files come from the core, which knows it
    // by the message it arrived as, and the page needs a `Sailfish.WebView`
    // that costs itself rather than the conversation if it is missing.
    function openApp(messageId) {
        pageStack.push(Qt.resolvedUrl("WebxdcPage.qml"), {
            accountId: page.accountId,
            messageId: messageId
        })
    }

    function sendCurrentText() {
        // The model refuses a second send while one is outstanding and the
        // button is disabled meanwhile; this says so a third time,
        // cheaply, since a double send is a message the reader cannot
        // take back.
        if (messages.sending) {
            return
        }
        // A recording under way is what the button sends.
        if (voiceBar.recording) {
            voiceBar.send()
            return
        }
        // A message being edited: the change goes to the core, and the
        // field is put back once it has taken it. The reply and the
        // file, put aside for the edit, are not part of it.
        if (page.editing) {
            var changed = textField.text.trim()
            if (changed.length > 0) {
                page.errorMessage = ""
                messages.edit_message(page.editingMessageId, changed)
            }
            return
        }
        // The bars are cleared from `onSent`, with the model's own copy:
        // clearing them here would drop the reply and the file the reader
        // chose on a send that never happened.
        //
        // Trimmed: a message of nothing but whitespace is not a message,
        // and a trailing newline from the keyboard is not part of one.
        var text = textField.text.trim()
        if (page.attachmentPath.length > 0) {
            // The button is already off and the bar says why; this says
            // so a second time, as the check on `sending` above does.
            if (messages.attachment_too_big) {
                return
            }
            page.errorMessage = ""
            messages.send_file(text, page.attachmentPath)
        } else if (text.length > 0) {
            page.errorMessage = ""
            messages.send(text)
        }
    }
}
