import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * One chat's messages. Own component: ConversationPage will not load headless.
 *
 * Follows arrivals only at the bottom, never pulling reader from history.
 */
SilicaListView {
    id: root

    // Groups show sender; one-to-one chats do not.
    property bool showSender: false

    /// Message the "new messages" line sits above; 0 for none.
    /// Model reads it from core on open, never moves it. See
    /// `ChatMessages.unread_from`.
    property int unreadFrom: 0
    property string placeholderText
    /// Body mode: 0 Markdown, else as written. Page binds from setting.
    property int markdownMode: 1
    /// Own message editable: chat takes messages and is encrypted.
    /// Row adds whether core lets reader edit this one.
    property bool canEdit: false
    /// Webxdc apps on: off draws `.xdc` as a file. Page binds from setting.
    property bool appsEnabled: false

    property bool stickToBottom: true

    /// View moved: rows want filling.
    ///
    /// Model holds a row per message; fills only those looked at. Raised, not
    /// acted on: component knows no core.
    signal hydrateRequested(int first, int last)

    /// Fill in on-screen rows.
    ///
    /// `indexAt` misses gaps: walk the view. Debounced, else flicks ask every frame.
    function askForRows() {
        var first = -1
        var last = -1
        for (var y = 1; y < root.height; y += Theme.paddingLarge) {
            var index = root.indexAt(root.width / 2, root.contentY + y)
            if (index >= 0) {
                if (first < 0) {
                    first = index
                }
                last = index
            }
        }
        if (first >= 0) {
            root.hydrateRequested(first, last)
        }
    }

    // Short: scroll-to-top lands in one step. Long enough to merge a flick's frames.
    Timer {
        id: fillRows
        interval: 60
        onTriggered: root.askForRows()
    }

    /// Ask for rows soon, unless already queued.
    ///
    /// Not restarted: a per-frame restart would never fire mid-flick. Fires every 60 ms.
    function askSoon() {
        if (!fillRows.running) {
            fillRows.start()
        }
    }

    // Count bound by page: `count` lags layout; arrivals count regardless.
    property int messageCount: 0
    // Arrivals since scroll-away, from model reports (`messageCount` also moves on deletes).
    property int missedCount: 0

    /// Messages from others just added.
    function noteArrivals(count) {
        if (!root.following) {
            root.missedCount += count
        }
    }

    // Raised, not acted on: component knows no core, so it loads alone.
    signal replyRequested(int messageId, string body, string author)
    /// Edit asked. Text travels: row may be gone by answer.
    signal editRequested(int messageId, string body)
    signal copyRequested(string body)
    /// Wait up; goes now. Own devices only, or everyone's, as picked.
    signal deleteRequested(int messageId, bool forEveryone)
    /// Delete asked, kind not chosen. Dialog in pages/DeleteMessageDialog.qml.
    /// `canDeleteForEveryone` travels: row may be gone by the answer.
    signal deleteChoiceRequested(int messageId, bool canDeleteForEveryone)
    signal resendRequested(int messageId)
    signal forwardRequested(int messageId)
    /// Attachment tapped; opening is the page's call.
    signal openRequested(url fileUrl, string fileName, string viewType,
                         real previewWidth)
    /// Keep-a-copy asked; page decides where. Copy takes sender's name.
    signal saveRequested(url fileUrl, string fileName)
    /// Full-text page asked. Author travels: row may be gone by build.
    signal fullTextRequested(int messageId, string author)
    /// Rest of a message held back by the download limit.
    signal downloadRequested(int messageId)
    /// Webxdc app tapped. Running it is the page's push.
    signal appRequested(int messageId)
    /// Call tapped, with state: page tells ringing from callback-able.
    signal callRequested(int messageId, bool outgoing, string callState, bool hasVideo)
    /// Emoji picked: model decides add or remove.
    signal reactionRequested(int messageId, string emoji)
    /// Emoji not in quick reactions: picker page. `current` is own reaction, "" for none.
    signal reactionPickerRequested(int messageId, string current)

    /// Milliseconds a message waits before it goes. Tests turn it down.
    property alias pendingDelay: doomedMessages.delay

    /// Send all waiting deletes now. Page calls on exit.
    function flushDeletes() {
        doomedMessages.flush()
    }

    /// Whether message waits to go. For tests.
    function pendingFor(messageId) {
        return doomedMessages.pending(messageId)
    }

    /// Delete kind picked; wait starts now, row shows countdown off `doomed` (the
    /// menu's row may have been rebuilt while the dialog was up).
    function confirmDelete(messageId, forEveryone) {
        doomedMessages.ask(messageId, forEveryone === true)
    }

    /// Messages awaiting delete, within the "did not mean it" window.
    ///
    /// Not Silica's `remorseAction`: it sits on the row, which deletes destroy (a
    /// run of deletes lost all but the first). See PendingRemoval.
    PendingRemoval {
        id: doomedMessages
        // `tag` is the page's pick: true deletes other ends' copies too.
        onRemove: root.deleteRequested(id, tag === true)
    }

    /// Quick reactions. "+" opens picker (pages/ReactionPickerPage.qml).
    /// Another's reaction chip: tap answers in kind.
    readonly property var quickReactions: ["👍", "❤️", "😂", "😮", "😢", "🙏"]

    /// Picture and video open on own page (PicturePage, VideoPage), not the system;
    /// those pages offer Open and Save, so the row menu omits them.
    /// ConversationPage.openAttachment pushes the page.
    function opensOnItsOwnPage(viewType) {
        return viewType === "Image" || viewType === "Gif"
               || viewType === "Sticker" || viewType === "Video"
    }

    /// Back to newest message, following again.
    function jumpToNewest() {
        // Over the list, so a tap does not stop a flick; left running, `held` stays true
        // and the scroll never happens.
        root.cancelFlick()
        root.held = false
        // Also let go of the held row, else the picture-return hold undoes the jump.
        root.releaseRow()
        root.stickToBottom = true
        root.missedCount = 0
        toEnd.restart()
        // Last: a tap does not reach an open context menu, which would block the jump.
        root.closeOpenMenu()
    }

    // Delegates draw outside the box unless clipped; what sits below is translucent.
    clip: true

    // Never straight away: model handed in is filled but unmeasured; end known after next pass.
    Timer {
        id: toEnd
        interval: 0
        // Checked again: reader may scroll away between the arrival and this pass.
        onTriggered: {
            if (root.following && !root.menuOpen) {
                root.positionViewAtEnd()
                root.reachedEnd = true
            }
        }
    }

    /// Whether view was ever put at newest. Before that, a far view means chat not yet opened.
    property bool reachedEnd: false

    // Tracked from drag/flick start to end. Not `moving`: tests cannot set that.
    property bool held: false

    // Not while reader holds it: rows measured in view grow `contentHeight`; following would haul
    // them back.
    readonly property bool following: root.stickToBottom && !root.held

    /// Row whose context menu is open; null for none.
    ///
    /// Long press is no drag, so scroll logic never knew. Silica scrolls to fit the menu: the view
    /// is left alone while one is up. Typed Item: a destroyed row clears it.
    property Item openMenuRow: null
    readonly property bool menuOpen: root.openMenuRow !== null

    /// Menu opened: held row released, as on a drag (the reader asked for the move).
    function menuOpened(row) {
        root.openMenuRow = row
        root.releaseRow()
    }

    /// Close open menu, stop waiting on it.
    ///
    /// Cleared first: a row closing later has nothing to answer.
    function closeOpenMenu() {
        var row = root.openMenuRow
        if (!row) {
            return
        }
        root.openMenuRow = null
        row.closeMenu()
    }

    /// Row's context menu closed.
    function menuClosed(row) {
        // Only the opening row: a second menu closes the first, in no fixed order.
        if (root.openMenuRow !== row) {
            return
        }
        root.openMenuRow = null
        // Row shrinks as the menu folds; a following view returns to the end, else sits short of
        // it.
        if (root.following) {
            toEnd.restart()
        }
    }

    // Taking hold of the list takes it over: held row lets go.
    onMovementStarted: {
        root.held = true
        root.releaseRow()
    }

    // Arrival moves `count`; measuring moves `contentHeight`: separate steps.
    onMessageCountChanged: if (root.following && !root.menuOpen) toEnd.restart()
    onContentHeightChanged: {
        // Unfolding grows content under a watching reader; hold and follow wait until it closes.
        if (root.menuOpen) {
            root.askSoon()
            return
        }
        // Held row first: content height changing is what moves reader off it.
        if (root.pendingRow >= 0) {
            root.putBack()
        } else if (root.following) {
            toEnd.restart()
        }
        // Opening may never move contentY; this asks for the first screen anyway.
        root.askSoon()
    }

    // View's position before the change, so a move is told from content growth.
    property real lastContentY: 0

    onContentYChanged: {
        var movedUp = root.contentY < root.lastContentY
        root.lastContentY = root.contentY
        // Page away: nobody scrolls, so any move is the list losing place. Put it back this turn.
        if (root.away) {
            if (root.pendingRow >= 0) {
                root.putBack()
            } else if (root.stickToBottom && !root.restoring) {
                root.restoring = true
                root.positionViewAtEnd()
                root.restoring = false
            }
            return
        }
        // Placeholders are one height: scrolling them never grows `contentHeight`; ask for them
        // here.
        root.askSoon()
        // Moved up far from newest untouched: system scroll-to-top. Following would haul reader
        // back. Excluded: drag/flick (`held`), own jump (`pendingRow`), chat still opening
        // (`reachedEnd`).
        //
        // Only up: view also moves down by itself (picture decode, end clamp); taking that as
        // leaving put a tall picture at chat end on show from top. Silica's menu-fit scroll moves
        // view up too.
        if (movedUp && root.reachedEnd && root.stickToBottom && !root.held
                && !root.restoring && root.pendingRow < 0 && !root.nearBottom
                && !root.menuOpen) {
            root.stickToBottom = false
        }
    }

    /// Message a search sent reader to; flashed once. 0 for none.
    property int foundMessageId: 0

    /// Put row mid-view and stay there. `stickToBottom` goes off first, or arrivals drag view back.
    function jumpToRow(index) {
        if (index < 0) {
            return
        }
        root.stickToBottom = false
        root.positionViewAtIndex(index, ListView.Center)
    }

    /// Put view on a row and keep it there while rows settle.
    ///
    /// One `positionViewAtIndex` is only right at final row heights. Rows measure as laid out, so
    /// hold: re-applied on each content change until reader takes over.
    function holdAt(index) {
        root.hold(index, false, 0)
    }

    /// Keep row where it is: top `offset` below view top, not centred.
    ///
    /// Holding at centre jumped on swipe-back: the reader stops where the thumb leaves the list.
    function holdPlace(index, offset) {
        root.hold(index, true, offset)
    }

    function hold(index, placed, offset) {
        if (index < 0) {
            return
        }
        root.stickToBottom = false
        root.pendingRow = index
        root.pendingPlaced = placed
        root.pendingOffset = offset
        root.putBack()
        holdDeadline.restart()
    }

    /// Let go of held row: reader took over, or view stopped moving under them.
    function releaseRow() {
        holdDeadline.stop()
        root.away = false
        root.pendingRow = -1
    }

    /// Row a jump holds the view on; -1 for none.
    property int pendingRow: -1
    /// Held where: top `pendingOffset` below view top if `pendingPlaced`, else centre.
    property bool pendingPlaced: false
    property real pendingOffset: 0
    /// True during `putBack`, so its own move is not another reason to move.
    property bool restoring: false

    function putBack() {
        // Not under an open menu: a re-armed hold would drag the menu off screen.
        if (root.restoring || root.menuOpen || root.pendingRow < 0) {
            return
        }
        root.restoring = true
        if (root.pendingPlaced) {
            root.placeRow(root.pendingRow, root.pendingOffset)
        } else {
            // First row has nothing above to centre against; "top" means beginning.
            root.positionViewAtIndex(
                root.pendingRow,
                root.pendingRow === 0 ? ListView.Beginning : ListView.Center)
        }
        root.restoring = false
    }

    /// Put row `index` with top `offset` below view top.
    ///
    /// No move if already there (`positionViewAtIndex` cannot promise that): find row
    /// on screen, bring it on only if absent, then move by exactly the difference.
    function placeRow(index, offset) {
        var item = root.itemOf(index)
        if (!item) {
            root.positionViewAtIndex(index, ListView.Beginning)
            item = root.itemOf(index)
        }
        if (item) {
            root.contentY = root.withinContent(item.y - offset)
        }
    }

    /// Item drawing row `index`, if on screen. Walked like `askForRows` (`itemAt` misses gaps).
    function itemOf(index) {
        for (var y = 0; y < root.height; y += Theme.paddingLarge) {
            if (root.indexAt(root.width / 2, root.contentY + y) === index) {
                return root.itemAt(root.width / 2, root.contentY + y)
            }
        }
        return undefined
    }

    /// `y` clamped to content ends, as a drag would.
    function withinContent(y) {
        var top = root.originY
        var bottom = Math.max(top, root.contentHeight + root.originY - root.height)
        return Math.min(Math.max(y, top), bottom)
    }

    // Let go only when reader takes view over, or on this deadline; shorter timers expire between
    // passes.
    Timer {
        id: holdDeadline
        interval: 6000
        onTriggered: root.releaseRow()
    }

    /// Reader's place before another page covers this: row (not pixel) + top offset, so
    /// return is to the pixel. Rows keep height under a page, but a reload or removed row
    /// can still lose the place.
    property int rememberedRow: -1
    property real rememberedOffset: 0
    property bool rememberedFollowing: false
    /// True while page away: row held with no deadline.
    property bool away: false

    function rememberPlace() {
        root.rememberedFollowing = root.stickToBottom
        root.rememberedRow = -1
        root.rememberedOffset = 0
        // First row at or below view middle. A single `indexAt` probe misses gaps.
        for (var y = root.height / 2; y < root.height; y += Theme.paddingLarge) {
            var index = root.indexAt(root.width / 2, root.contentY + y)
            if (index >= 0) {
                root.rememberedRow = index
                root.rememberedOffset =
                    root.itemAt(root.width / 2, root.contentY + y).y - root.contentY
                break
            }
        }
        // Armed, not written down: a list losing place while away paints the top for a frame.
        // Armed, the loss is undone in the same turn; a following view returns to end.
        root.away = true
        if (root.rememberedRow >= 0 && !root.rememberedFollowing) {
            root.pendingRow = root.rememberedRow
            root.pendingPlaced = true
            root.pendingOffset = root.rememberedOffset
            holdDeadline.stop()
        }
    }

    function restorePlace() {
        // Hold armed on the way out ends here; the deadline applies again.
        root.away = false
        if (root.rememberedFollowing) {
            // Following view kept at end while away: resending it moved readers a line short, every
            // return.
            root.releaseRow()
        } else if (root.rememberedRow >= 0) {
            // Held again, not positioned: return is a relayout and rows settle after it.
            root.holdPlace(root.rememberedRow, root.rememberedOffset)
        } else {
            root.releaseRow()
        }
        root.rememberedRow = -1
    }

    // Near end, not `atYEnd`: `contentHeight` still grows as a scroll stops. A line short counts.
    readonly property real bottomSlack: Theme.itemSizeLarge

    /// At or near newest, including a chat too short to scroll.
    readonly property bool nearBottom:
        root.contentHeight <= root.height
        || root.contentY + root.height
           >= root.contentHeight + root.originY - root.bottomSlack

    // Where reader left off, once they stop moving.
    onMovementEnded: {
        root.held = false
        root.stickToBottom = root.nearBottom
        if (root.nearBottom) {
            root.missedCount = 0
        }
    }

    // Arriving at newest, by scrolling or the button, counts as having read what is there.
    onStickToBottomChanged: if (root.stickToBottom) root.arrivedAtNewest()
    signal arrivedAtNewest()


    // Model counts days in viewer timezone: grouping by that number splits days.
    //
    // No `section.delegate` (own item, sized from last measured height, drew the heading over
    // the message beneath). Heading drawn inside the row (`dayHeading`).
    section.property: "day_number"

    delegate: ListItem {
        id: messageRow
        objectName: "messageRow"

        // View moves itself (follow, hold). A menu unfolding grows content and makes Silica
        // scroll, what both watch for; only the row can say which is happening.
        onMenuOpenChanged: {
            if (messageRow.menuOpen) {
                root.menuOpened(messageRow)
            } else {
                root.menuClosed(messageRow)
            }
        }

        // Silica builds it on first open. Declared here, six reactions, "+" and eight items (30+
        // objects) would cost every row, and every flick frame.
        menu: Component {
            ContextMenu {
                id: rowMenu

                // Quick reactions above actions: one tap ends the menu. Not MenuItem (one line).
                Item {
                    id: reactionPicker
                    objectName: "reactionPicker"
                    // Core notice is no one's message to react to.
                    readonly property bool shown: !model.is_info
                    visible: reactionPicker.shown
                    width: parent ? parent.width : 0
                    height: reactionPicker.shown ? Theme.itemSizeSmall : 0
                    /// Taken while row is here, like Delete's id: menu can outlive its row.
                    readonly property int messageId: model.message_id
                    /// Reader's own reaction, for picker to mark; taken now for the same reason.
                    readonly property string myReaction: model.my_reaction
                    // Emoji and "+" side by side, centred, narrower than a small item if need be.
                    // Hand-laid, not a Row (see MessageDelegate).
                    readonly property int slots: root.quickReactions.length + 1
                    readonly property real slotGap: Theme.paddingSmall
                    readonly property real slotSize: Math.min(
                        Theme.itemSizeSmall,
                        Math.floor((width - (slots - 1) * slotGap) / slots))
                    readonly property real slotsX: (width - slots * slotSize
                                                    - (slots - 1) * slotGap) / 2

                    Repeater {
                        model: root.quickReactions

                        MouseArea {
                            objectName: "reactionOption"
                            x: reactionPicker.slotsX
                               + index * (reactionPicker.slotSize + reactionPicker.slotGap)
                            width: reactionPicker.slotSize
                            height: reactionPicker.height
                            readonly property string emoji: modelData
                            function choose() {
                                root.reactionRequested(reactionPicker.messageId, emoji)
                                rowMenu.close()
                            }
                            onClicked: choose()

                            EmojiGlyph {
                                anchors.centerIn: parent
                                size: Theme.fontSizeLarge
                                emoji: modelData
                            }
                        }
                    }

                    // Every other emoji: the picker, which is a page.
                    MouseArea {
                        id: morePicker
                        objectName: "reactionMore"
                        x: reactionPicker.slotsX + root.quickReactions.length
                           * (reactionPicker.slotSize + reactionPicker.slotGap)
                        width: reactionPicker.slotSize
                        height: reactionPicker.height
                        function choose() {
                            root.reactionPickerRequested(reactionPicker.messageId,
                                                         reactionPicker.myReaction)
                            rowMenu.close()
                        }
                        onClicked: choose()

                        Image {
                            anchors.centerIn: parent
                            source: "image://theme/icon-m-add"
                                    + (morePicker.pressed ? "?" + Theme.highlightColor : "")
                        }
                    }
                }

                MenuItem {
                    objectName: "replyItem"
                    // Core notice is nobody's message to answer.
                    visible: !model.is_info
                    text: qsTr("Reply")
                    onClicked: root.replyRequested(model.message_id, model.text,
                                                   model.sender_name)
                }
                MenuItem {
                    objectName: "editItem"
                    // As deltachat-android: own text message, not notice or call, not cut by the
                    // sending core.
                    visible: root.canEdit && model.is_outgoing && !model.is_info
                             && model.text.length > 0 && !model.has_html
                             && model.view_type !== "Call"
                    text: qsTr("Edit")
                    onClicked: root.editRequested(model.message_id, model.text)
                }
                MenuItem {
                    objectName: "copyItem"
                    // Copying a captionless image emptied the clipboard and said it worked.
                    visible: model.text.length > 0
                    text: qsTr("Copy")
                    onClicked: root.copyRequested(model.text)
                }
                MenuItem {
                    objectName: "openItem"
                    // Only messages with a file. Webxdc runs (own tap); with apps off, .xdc is a
                    // file. Pictures and videos have their own page with Open, so omitted here.
                    visible: model.file_path.length > 0
                             && !root.opensOnItsOwnPage(model.view_type)
                             && !(root.appsEnabled
                                  && model.view_type === "Webxdc")
                    text: qsTr("Open")
                    onClicked: root.openRequested(
                                   "file://" + model.file_path, model.file_name,
                                   model.view_type, 0)
                }
                MenuItem {
                    objectName: "saveItem"
                    // Own copy outside the app. Skipped where a page has Open.
                    visible: model.file_path.length > 0
                             && !root.opensOnItsOwnPage(model.view_type)
                    text: qsTr("Save")
                    onClicked: root.saveRequested("file://" + model.file_path,
                                                  model.file_name)
                }
                MenuItem {
                    objectName: "forwardItem"
                    // Core notice is not the reader's to pass on.
                    visible: !model.is_info
                    text: qsTr("Forward")
                    // Taken now, not in the callback: row may be gone when the chat is picked.
                    onClicked: root.forwardRequested(model.message_id)
                }
                MenuItem {
                    objectName: "resendItem"
                    // DC_STATE_OUT_FAILED: the only state worth retrying.
                    visible: model.state === 24
                    text: qsTr("Send again")
                    onClicked: root.resendRequested(model.message_id)
                }
                MenuItem {
                    objectName: "downloadItem"
                    // Download states; the row offers the same tap.
                    visible: model.download_state === "Available"
                             || model.download_state === "Failure"
                    text: qsTr("Download")
                    onClicked: root.downloadRequested(model.message_id)
                }
                MenuItem {
                    objectName: "deleteItem"
                    text: qsTr("Delete")
                    // Kind asked in a dialog: side by side, a long press tapped the wrong one.
                    // Read now (row may be gone by the answer). Everyone only for own, encrypted.
                    onClicked: root.deleteChoiceRequested(
                                   model.message_id,
                                   model.is_outgoing && model.show_padlock
                                   && !model.is_info)
                }
            }
        }
        // Sized by content: fixed height overlaps wrapped messages. Unfetched row is one line
        // (gives list a length before fetch). Day heading is part of the height.
        contentHeight: dayHeading.height + unreadLine.height
                       + (model.loaded ? body.height : Theme.itemSizeExtraSmall)

        /// Message is on its way out.
        readonly property bool doomed: doomedMessages.pending(model.message_id)

        /// Countdown already up: answer and rebuild both raise it. A second `execute` resets it.
        property bool countingDown: false

        // Wait starts on the menu's page, or while the row was scrolled away. Row learns here.
        onDoomedChanged: {
            if (messageRow.doomed) {
                messageRow.raiseRemorse()
            } else {
                messageRow.countingDown = false
            }
        }

        /// Silica's own countdown over the message: bar, seconds, "Tap to cancel".
        ///
        /// Deletion is `doomedMessages`' job: remorse lives in the row, which deletes destroy.
        /// Callback is a no-op; this only draws and reports the tap. Needs `body`: a message
        /// built later puts it up on load.
        function raiseRemorse() {
            if (messageRow.countingDown || !body.item) {
                return
            }
            messageRow.countingDown = true
            remorse.active = true
            //: What Silica's countdown says it is doing, over a
            //: message the reader has asked to delete.
            remorse.item.execute(
                body.item, qsTr("Deleting"), function() {},
                doomedMessages.countdownFor(model.message_id))
        }

        // Built on first delete: a dozen items, one per row for a rare tap otherwise.
        Loader {
            id: remorse
            active: false
            sourceComponent: RemorseItem {
                objectName: "messageRemorse"
                onCanceled: doomedMessages.spare(model.message_id)
            }
        }

        // Rows rebuild on scroll-back: one scrolled past mid-wait returns with no countdown.
        Component.onCompleted: {
            if (messageRow.doomed) {
                messageRow.raiseRemorse()
            }
        }

        // One surface: tap opens what the message opens, long press the menu.
        // Row takes the press. Remorse covers a message waiting to go, taking the tap itself.
        // Placeholder: nothing to open.
        onClicked: {
            if (body.item) {
                body.item.tapped()
            }
        }

        /// Day's date, on the first row of each day. Inside the row (see `section.property`).
        Label {
            id: dayHeading
            objectName: "dayLabel"
            width: parent.width
            // Filled from `section.property`: first row of a day differs from the one before; ""
            // before the first counts as a change. Day 0: core sent no day markers; no heading
            // (epoch would be worse). Attached to the
            // delegate root (`ListView` attaches to its item): outside a view both read undefined,
            // so false. Sized by that, not `visible` (see MessageDelegate).
            readonly property bool shown: messageRow.ListView.section !== "0"
                                          && messageRow.ListView.section
                                             !== messageRow.ListView.previousSection
            visible: dayHeading.shown
            height: dayHeading.shown ? implicitHeight + Theme.paddingMedium : 0
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            // Day number: read its UTC date, rebuild as local with the same parts. No offset
            // maths: it would bring back the DST error the model avoids.
            text: {
                var day = parseInt(messageRow.ListView.section, 10)
                if (isNaN(day)) {
                    return ""
                }
                var utc = new Date(day * 86400000)
                return Qt.formatDate(new Date(utc.getUTCFullYear(),
                                              utc.getUTCMonth(),
                                              utc.getUTCDate(), 12),
                                     Qt.DefaultLocaleLongDate)
            }
        }

        /// Where reader left off: everything below arrived while away.
        ///
        /// Inside the row, under the day heading when both ("Tuesday, and here is what is new").
        Item {
            id: unreadLine
            objectName: "unreadLine"
            width: parent.width
            y: dayHeading.height
            readonly property bool shown: root.unreadFrom > 0
                                          && model.message_id === root.unreadFrom
            visible: unreadLine.shown
            height: unreadLine.shown ? unreadLabel.implicitHeight + 2 * Theme.paddingMedium : 0

            Rectangle {
                anchors {
                    left: parent.left
                    leftMargin: Theme.horizontalPageMargin
                    right: unreadLabel.left
                    rightMargin: Theme.paddingMedium
                    verticalCenter: unreadLabel.verticalCenter
                }
                height: 1
                color: Theme.rgba(Theme.highlightColor, 0.4)
            }

            Label {
                id: unreadLabel
                objectName: "unreadLabel"
                anchors.centerIn: parent
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.highlightColor
                textFormat: Text.PlainText
                //: The line in a conversation above the first message that
                //: arrived while the reader was away.
                text: qsTr("New messages")
            }

            Rectangle {
                anchors {
                    left: unreadLabel.right
                    leftMargin: Theme.paddingMedium
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: unreadLabel.verticalCenter
                }
                height: 1
                color: Theme.rgba(Theme.highlightColor, 0.4)
            }
        }

        // Built only once fetched: a placeholder must not draw (or build) a message it lacks; a
        // message is ~36 items, and a flick through unfetched history builds a screenful of
        // placeholders per frame. Loader takes the message's height; hands it the row's width.
        Loader {
            id: body
            y: dayHeading.height + unreadLine.height
            width: parent.width
            active: model.loaded
            // Message arriving into a counting-down row is covered at once.
            onLoaded: {
                if (messageRow.doomed) {
                    messageRow.raiseRemorse()
                }
            }
            sourceComponent: MessageDelegate {
                objectName: "messageDelegate"
                // Not hidden: remorse fades it. Hiding would hide children, and every part measures
                // `visible ? implicitHeight : 0`, so the row would collapse under the countdown.
                enabled: !messageRow.doomed
                messageText: model.text
                styledText: model.styled_text
                isEdited: model.is_edited
                markdownMode: root.markdownMode
                downloadState: model.download_state
                isOutgoing: model.is_outgoing
                isInfo: model.is_info
                isForwarded: model.is_forwarded
                isFound: root.foundMessageId === model.message_id
                showPadlock: model.show_padlock
                deliveryState: model.state
                sentAt: model.timestamp
                senderName: model.sender_name
                senderColor: model.sender_color
                showSender: root.showSender
                quoteText: model.quote_text
                quoteAuthor: model.quote_author
                filePath: model.file_path
                fileName: model.file_name
                fileMime: model.file_mime
                fileBytes: model.file_bytes
                durationMs: model.duration_ms
                viewType: model.view_type
                imageWidth: model.image_width
                imageHeight: model.image_height
                isNew: model.is_new
                hasHtml: model.has_html
                vcardName: model.vcard_name
                vcardAddr: model.vcard_addr
                vcardColor: model.vcard_color
                webxdcName: model.webxdc_name
                webxdcDocument: model.webxdc_document
                webxdcSummary: model.webxdc_summary
                webxdcIcon: model.webxdc_icon
                appsEnabled: root.appsEnabled
                scrolling: root.held
                callState: model.call_state
                callHasVideo: model.call_has_video
                callDuration: model.call_duration
                reactions: model.reactions
                onOpenRequested: root.openRequested(fileUrl, fileName, viewType,
                                                    previewWidth)
                onFullTextRequested: root.fullTextRequested(model.message_id,
                                                           model.sender_name)
                onAppRequested: root.appRequested(model.message_id)
                onCallRequested: root.callRequested(model.message_id,
                                                    model.is_outgoing,
                                                    model.call_state,
                                                    model.call_has_video)
                onDownloadRequested: root.downloadRequested(model.message_id)
                onReactionRequested: root.reactionRequested(model.message_id, emoji)
                // Long press on a chip, download offer or play button: goes to the row.
                onMenuRequested: messageRow.openMenu()
            }
        }
    }

    /// Whether messages were fetched yet. Without it, every open flashed "no messages yet".
    property bool loaded: true

    ViewPlaceholder {
        objectName: "emptyPlaceholder"
        enabled: root.loaded && root.count === 0
        text: root.placeholderText
    }
}
