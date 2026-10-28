import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * One chat's messages; own component as ConversationPage won't load headless.
 * Follows arrivals only at bottom, never pulling reader from history.
 */
SilicaListView {
    id: root

    // Groups show sender; one-to-one chats do not.
    property bool showSender: false

    /// Message under the "new messages" line; 0 none. Read from core on open,
    /// never moved (`ChatMessages.unread_from`).
    property int unreadFrom: 0
    property string placeholderText
    /// 0 Markdown, else as written; bound from setting.
    property int markdownMode: 1
    /// Own messages editable: chat writable, encrypted. Row adds core's per-message check.
    property bool canEdit: false
    /// Webxdc apps on; off draws `.xdc` as file. Bound from setting.
    property bool appsEnabled: false

    property bool stickToBottom: true

    /// View moved: rows want filling (model fills only rows looked at). Component knows no core.
    signal hydrateRequested(int first, int last)

    /// Fill on-screen rows. Walks view (`indexAt` misses gaps); debounced, else flicks ask
    /// per frame.
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

    // Short enough for one-step scroll-to-top, long enough to merge flick frames.
    Timer {
        id: fillRows
        interval: 60
        onTriggered: root.askForRows()
    }

    /// Ask for rows soon unless queued. Not restarted: per-frame restart never fires mid-flick.
    function askSoon() {
        if (!fillRows.running) {
            fillRows.start()
        }
    }

    // Bound by page: `count` lags layout.
    property int messageCount: 0
    // Arrivals since scroll-away, from model (`messageCount` moves on deletes too).
    property int missedCount: 0

    /// Messages from others just added.
    function noteArrivals(count) {
        if (!root.following) {
            root.missedCount += count
        }
    }

    // Raised only, so component loads without core.
    signal replyRequested(int messageId, string body, string author)
    /// Text travels: row may be gone by answer.
    signal editRequested(int messageId, string body)
    signal copyRequested(string body)
    /// Wait over: delete now, own devices or everyone's.
    signal deleteRequested(int messageId, bool forEveryone)
    /// Delete asked, kind unchosen (pages/DeleteMessageDialog.qml). Flag travels: row may go.
    signal deleteChoiceRequested(int messageId, bool canDeleteForEveryone)
    signal resendRequested(int messageId)
    signal forwardRequested(int messageId)
    /// Attachment tapped; page opens it. Message id: gallery swipes from it.
    signal openRequested(url fileUrl, string fileName, string viewType,
                         real previewWidth, int messageId)
    /// Page picks where; copy keeps sender's file name.
    signal saveRequested(url fileUrl, string fileName)
    /// Author travels: row may be gone by build.
    signal fullTextRequested(int messageId, string author)
    /// Rest of a message held back by the download limit.
    signal downloadRequested(int messageId)
    /// Webxdc app tapped; page runs it.
    signal appRequested(int messageId)
    /// State lets page tell ringing from callback-able.
    signal callRequested(int messageId, bool outgoing, string callState, bool hasVideo)
    /// Model decides add or remove.
    signal reactionRequested(int messageId, string emoji)
    /// Non-quick emoji: picker page. `current`: own reaction, "" none.
    signal reactionPickerRequested(int messageId, string current)
    /// Quote tapped. `quotedId` 0: core has no original (deleted, never here).
    signal quoteRequested(int quotedId)
    /// Row's location tapped: map of it, or sender's newest.
    signal locationRequested(int messageId, int senderId, string senderName)

    /// Delete wait, ms; tests lower it.
    property alias pendingDelay: doomedMessages.delay

    /// Send waiting deletes now; page calls on exit.
    function flushDeletes() {
        doomedMessages.flush()
    }

    /// Message waits to go? For tests.
    function pendingFor(messageId) {
        return doomedMessages.pending(messageId)
    }

    /// Kind picked: wait starts; row shows countdown off `doomed` (menu's row may be
    /// rebuilt under dialog).
    function confirmDelete(messageId, forEveryone) {
        doomedMessages.ask(messageId, forEveryone === true)
    }

    /// Messages in the undo window. Not Silica's `remorseAction`: it lives on the row,
    /// which deletes destroy (a run lost all but first). See PendingRemoval.
    PendingRemoval {
        id: doomedMessages
        // `tag` true: delete for everyone.
        onRemove: root.deleteRequested(id, tag === true)
    }

    /// "+" opens pages/ReactionPickerPage.qml. Tapping another's chip answers in kind.
    readonly property var quickReactions: ["👍", "❤️", "😂", "😮", "😢", "🙏"]

    /// Picture/video open in PicturePage/VideoPage (ConversationPage.openAttachment),
    /// which offer Open and Save; row menu omits them.
    function opensOnItsOwnPage(viewType) {
        return viewType === "Image" || viewType === "Gif"
               || viewType === "Sticker" || viewType === "Video"
    }

    /// Back to newest message, following again.
    function jumpToNewest() {
        // Button over list: tap doesn't stop a flick; left running, `held` stays true, no scroll.
        root.cancelFlick()
        root.held = false
        // Else picture-return hold undoes jump.
        root.releaseRow()
        root.stickToBottom = true
        root.missedCount = 0
        toEnd.restart()
        // Last: tap misses an open menu, which would block jump.
        root.closeOpenMenu()
    }

    // Delegates overdraw unless clipped; below is translucent.
    clip: true

    // Deferred: new model filled but unmeasured; end known next pass.
    Timer {
        id: toEnd
        interval: 0
        // Rechecked: reader may scroll away meanwhile.
        onTriggered: {
            if (root.following && !root.menuOpen) {
                root.positionViewAtEnd()
                root.reachedEnd = true
            }
        }
    }

    /// View ever put at newest; before, far view means chat still opening.
    property bool reachedEnd: false

    // Drag/flick in progress. Not `moving`: tests can't set it.
    property bool held: false

    // Off while held: in-view measuring grows `contentHeight`; following would haul back.
    readonly property bool following: root.stickToBottom && !root.held

    /// Row with open context menu, or null. Long press is no drag, so scroll logic is told;
    /// Silica scrolls to fit menu, so view left alone meanwhile. Typed Item: destroyed row clears.
    property Item openMenuRow: null
    readonly property bool menuOpen: root.openMenuRow !== null

    /// Releases held row, as a drag would.
    function menuOpened(row) {
        root.openMenuRow = row
        root.releaseRow()
    }

    /// Close open menu. Cleared first: late close answers nothing.
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
        // Opening row only: second menu closes first, order unfixed.
        if (root.openMenuRow !== row) {
            return
        }
        root.openMenuRow = null
        // Row shrinks as menu folds; following view returns to end.
        if (root.following) {
            toEnd.restart()
        }
    }

    // Grabbing list releases held row.
    onMovementStarted: {
        root.held = true
        root.releaseRow()
    }

    // Arrival moves `count`, measuring `contentHeight`: separate steps.
    onMessageCountChanged: if (root.following && !root.menuOpen) toEnd.restart()
    onContentHeightChanged: {
        // Menu unfolding grows content; hold and follow wait for close.
        if (root.menuOpen) {
            root.askSoon()
            return
        }
        // Held row first: height change is what moves reader off it.
        if (root.pendingRow >= 0) {
            root.putBack()
        } else if (root.following) {
            toEnd.restart()
        }
        // Opening may not move contentY; ask for first screen anyway.
        root.askSoon()
    }

    // Prior contentY, to tell a move from content growth.
    property real lastContentY: 0

    onContentYChanged: {
        var movedUp = root.contentY < root.lastContentY
        root.lastContentY = root.contentY
        // Page away: any move is list losing place; restore before return.
        if (root.away) {
            if (!root.restoring) {
                awayRestore.start()
            }
            return
        }
        // Placeholders share a height, so scrolling never grows `contentHeight`: ask here.
        root.askSoon()
        // Untouched move up, far from newest: system scroll-to-top; stop following. Excluded:
        // drag/flick (`held`), own jump (`pendingRow`), chat opening (`reachedEnd`), Silica
        // menu-fit scroll. Only up: view moves down alone (picture decode, end clamp); treating
        // that as leaving showed a tall end picture from its top.
        if (movedUp && root.reachedEnd && root.stickToBottom && !root.held
                && !root.restoring && root.pendingRow < 0 && !root.nearBottom
                && !root.menuOpen) {
            root.stickToBottom = false
        }
    }

    /// Search-target message, flashed once; 0 none.
    property int foundMessageId: 0

    /// Centre row and stay. `stickToBottom` off first, else arrivals drag view back.
    function jumpToRow(index) {
        if (index < 0) {
            return
        }
        root.stickToBottom = false
        root.positionViewAtIndex(index, ListView.Center)
    }

    /// Hold view on row while rows settle: one `positionViewAtIndex` is right only at final
    /// heights, so reapplied on each content change until reader takes over.
    function holdAt(index) {
        root.hold(index, false, 0)
    }

    /// Hold row top `offset` below view top, not centred: centring jumped on swipe-back.
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

    /// Release held row: reader took over or view settled.
    function releaseRow() {
        holdDeadline.stop()
        root.away = false
        root.pendingRow = -1
    }

    /// Held row; -1 none.
    property int pendingRow: -1
    /// `pendingPlaced`: top `pendingOffset` below view top; else centre.
    property bool pendingPlaced: false
    property real pendingOffset: 0
    /// True in `putBack`, so own move doesn't retrigger.
    property bool restoring: false

    function putBack() {
        // Not under open menu: hold would drag it off screen.
        if (root.restoring || root.menuOpen || root.pendingRow < 0) {
            return
        }
        root.restoring = true
        if (root.pendingPlaced) {
            root.placeRow(root.pendingRow, root.pendingOffset)
        } else {
            // Row 0: nothing above to centre against.
            root.positionViewAtIndex(
                root.pendingRow,
                root.pendingRow === 0 ? ListView.Beginning : ListView.Center)
        }
        root.restoring = false
    }

    /// Put row `index` top `offset` below view top; no move if already there
    /// (`positionViewAtIndex` can't promise that).
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

    /// On-screen item of row `index`; walked as `askForRows`.
    function itemOf(index) {
        for (var y = 0; y < root.height; y += Theme.paddingLarge) {
            if (root.indexAt(root.width / 2, root.contentY + y) === index) {
                return root.itemAt(root.width / 2, root.contentY + y)
            }
        }
        return undefined
    }

    /// `y` clamped to content ends.
    function withinContent(y) {
        var top = root.originY
        var bottom = Math.max(top, root.contentHeight + root.originY - root.height)
        return Math.min(Math.max(y, top), bottom)
    }

    // Hold ends on takeover or this deadline; shorter ones expired between passes.
    Timer {
        id: holdDeadline
        interval: 6000
        onTriggered: root.releaseRow()
    }

    /// Place before a page covers this: row + top offset, for pixel-exact return. Reload or
    /// removed row can still lose it.
    property int rememberedRow: -1
    property real rememberedOffset: 0
    property bool rememberedFollowing: false
    /// Page away: row held, no deadline.
    property bool away: false

    // Away restore after handler returns: `positionViewAtIndex` from inside `contentYChanged`
    // drops view's rows without building new ones (blank list, held row never found).
    Timer {
        id: awayRestore
        interval: 0
        onTriggered: {
            if (!root.away) {
                return
            }
            if (root.pendingRow >= 0) {
                root.putBack()
            } else if (root.stickToBottom) {
                root.restoring = true
                root.positionViewAtEnd()
                root.restoring = false
            }
        }
    }

    function rememberPlace() {
        root.rememberedFollowing = root.stickToBottom
        root.rememberedRow = -1
        root.rememberedOffset = 0
        // First row at/below mid-view; walked (one `indexAt` misses gaps).
        for (var y = root.height / 2; y < root.height; y += Theme.paddingLarge) {
            var index = root.indexAt(root.width / 2, root.contentY + y)
            if (index >= 0) {
                root.rememberedRow = index
                root.rememberedOffset =
                    root.itemAt(root.width / 2, root.contentY + y).y - root.contentY
                break
            }
        }
        // Hold armed now, not on return: lost place would paint top for a frame; armed, undone
        // while away.
        root.away = true
        if (root.rememberedRow >= 0 && !root.rememberedFollowing) {
            root.pendingRow = root.rememberedRow
            root.pendingPlaced = true
            root.pendingOffset = root.rememberedOffset
            holdDeadline.stop()
        }
    }

    function restorePlace() {
        // Away hold ends; deadline applies again.
        root.away = false
        if (root.rememberedFollowing) {
            // Kept at end while away; re-sending moved it a line short each return.
            root.releaseRow()
        } else if (root.rememberedRow >= 0) {
            // Held, not positioned: return relayouts, rows settle after.
            root.holdPlace(root.rememberedRow, root.rememberedOffset)
        } else {
            root.releaseRow()
        }
        root.rememberedRow = -1
    }

    // Not `atYEnd`: `contentHeight` grows as scroll stops. A line short counts.
    readonly property real bottomSlack: Theme.itemSizeLarge

    /// Near newest, or chat too short to scroll.
    readonly property bool nearBottom:
        root.contentHeight <= root.height
        || root.contentY + root.height
           >= root.contentHeight + root.originY - root.bottomSlack

    // Settle follow state on stop.
    onMovementEnded: {
        root.held = false
        root.stickToBottom = root.nearBottom
        if (root.nearBottom) {
            root.missedCount = 0
        }
    }

    // Reaching newest (scroll or button) counts as read.
    onStickToBottomChanged: if (root.stickToBottom) root.arrivedAtNewest()
    signal arrivedAtNewest()


    // Day number in viewer timezone. No `section.delegate`: sized from stale height, it overdrew
    // next message; heading lives in row (`dayHeading`).
    section.property: "day_number"

    delegate: ListItem {
        id: messageRow
        objectName: "messageRow"

        // Menu unfolding grows content and makes Silica scroll, which follow/hold watch for;
        // only the row knows.
        onMenuOpenChanged: {
            if (messageRow.menuOpen) {
                root.menuOpened(messageRow)
            } else {
                root.menuClosed(messageRow)
            }
        }

        // Built on first open; inline, its 30+ objects would cost every row each flick frame.
        menu: Component {
            ContextMenu {
                id: rowMenu

                // Reactions above actions, one tap closes. Not MenuItem (one line).
                Item {
                    id: reactionPicker
                    objectName: "reactionPicker"
                    // Not on core notices.
                    readonly property bool shown: !model.is_info
                    visible: reactionPicker.shown
                    width: parent ? parent.width : 0
                    height: reactionPicker.shown ? Theme.itemSizeSmall : 0
                    /// Read now: menu can outlive row.
                    readonly property int messageId: model.message_id
                    /// Own reaction for picker to mark; read now likewise.
                    readonly property string myReaction: model.my_reaction
                    // Emoji and "+" centred, shrunk if needed; hand-laid (see MessageDelegate).
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

                    // Other emoji: picker page.
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
                    // Not on core notices.
                    visible: !model.is_info
                    text: qsTr("Reply")
                    onClicked: root.replyRequested(model.message_id, model.text,
                                                   model.sender_name)
                }
                MenuItem {
                    objectName: "editItem"
                    // As deltachat-android: own text, not notice/call/truncated by sender core.
                    visible: root.canEdit && model.is_outgoing && !model.is_info
                             && model.text.length > 0 && !model.has_html
                             && model.view_type !== "Call"
                    text: qsTr("Edit")
                    onClicked: root.editRequested(model.message_id, model.text)
                }
                MenuItem {
                    objectName: "copyItem"
                    // Captionless image copy emptied clipboard, claimed success.
                    visible: model.text.length > 0
                    text: qsTr("Copy")
                    onClicked: root.copyRequested(model.text)
                }
                MenuItem {
                    objectName: "openItem"
                    // Files only; not pictures/videos (own page) or Webxdc while apps on.
                    visible: model.file_path.length > 0
                             && !root.opensOnItsOwnPage(model.view_type)
                             && !(root.appsEnabled
                                  && model.view_type === "Webxdc")
                    text: qsTr("Open")
                    onClicked: root.openRequested(
                                   "file://" + model.file_path, model.file_name,
                                   model.view_type, 0, model.message_id)
                }
                MenuItem {
                    objectName: "saveItem"
                    // Copy outside app; skipped where own page offers it.
                    visible: model.file_path.length > 0
                             && !root.opensOnItsOwnPage(model.view_type)
                    text: qsTr("Save")
                    onClicked: root.saveRequested("file://" + model.file_path,
                                                  model.file_name)
                }
                MenuItem {
                    objectName: "forwardItem"
                    // Not on core notices.
                    visible: !model.is_info
                    text: qsTr("Forward")
                    // Id read now: row may be gone by chat pick.
                    onClicked: root.forwardRequested(model.message_id)
                }
                MenuItem {
                    objectName: "resendItem"
                    // 24 = DC_STATE_OUT_FAILED, only retryable state.
                    visible: model.state === 24
                    text: qsTr("Send again")
                    onClicked: root.resendRequested(model.message_id)
                }
                MenuItem {
                    objectName: "downloadItem"
                    // Row offers same tap.
                    visible: model.download_state === "Available"
                             || model.download_state === "Failure"
                    text: qsTr("Download")
                    onClicked: root.downloadRequested(model.message_id)
                }
                MenuItem {
                    objectName: "deleteItem"
                    text: qsTr("Delete")
                    // Kind asked in dialog: side-by-side items got mis-tapped. Read now; everyone
                    // only for own encrypted.
                    onClicked: root.deleteChoiceRequested(
                                   model.message_id,
                                   model.is_outgoing && model.show_padlock
                                   && !model.is_info)
                }
            }
        }
        // Content-sized (fixed overlapped wraps), day heading included. Unfetched: one line,
        // giving list a length.
        contentHeight: dayHeading.height + unreadLine.height
                       + (model.loaded ? body.height : Theme.itemSizeExtraSmall)

        /// Pending delete.
        readonly property bool doomed: doomedMessages.pending(model.message_id)

        /// Countdown up: answer and rebuild both raise it; second `execute` resets it.
        property bool countingDown: false

        // Wait may start under dialog or while scrolled away; row learns here.
        onDoomedChanged: {
            if (messageRow.doomed) {
                messageRow.raiseRemorse()
            } else {
                messageRow.countingDown = false
            }
        }

        /// Silica countdown over message; only draws and reports cancel (no-op callback):
        /// `doomedMessages` deletes. Needs `body`; later load raises it.
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

        // Built on first delete: else a dozen items per row.
        Loader {
            id: remorse
            active: false
            sourceComponent: RemorseItem {
                objectName: "messageRemorse"
                onCanceled: doomedMessages.spare(model.message_id)
            }
        }

        // Rebuilt on scroll-back mid-wait: re-raise countdown.
        Component.onCompleted: {
            if (messageRow.doomed) {
                messageRow.raiseRemorse()
            }
        }

        // Row takes press: tap opens message, long press menu. Remorse takes taps while
        // pending; placeholder opens nothing.
        onClicked: {
            if (body.item) {
                body.item.tapped()
            }
        }

        /// Date on each day's first row.
        Label {
            id: dayHeading
            objectName: "dayLabel"
            width: parent.width
            // On section change ("" before first counts). Day 0: core sent no day markers, no
            // heading (epoch worse). Attached on delegate root; outside a view undefined, so
            // false. Sized by this, not `visible` (see MessageDelegate).
            readonly property bool shown: messageRow.ListView.section !== "0"
                                          && messageRow.ListView.section
                                             !== messageRow.ListView.previousSection
            visible: dayHeading.shown
            height: dayHeading.shown ? implicitHeight + Theme.paddingMedium : 0
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            // UTC date parts rebuilt as local; offset maths would bring back the DST error
            // model avoids.
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

        /// Below: arrived while away. In row, under day heading if both.
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

        // Built once fetched: message is ~36 items; flicking unfetched history makes a screenful
        // of placeholders per frame. Takes message height, gives row width.
        Loader {
            id: body
            y: dayHeading.height + unreadLine.height
            width: parent.width
            active: model.loaded
            // Cover late-loaded pending message.
            onLoaded: {
                if (messageRow.doomed) {
                    messageRow.raiseRemorse()
                }
            }
            sourceComponent: MessageDelegate {
                objectName: "messageDelegate"
                // Disabled, not hidden: parts size by `visible`, so row would collapse under
                // countdown.
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
                hasLocation: model.has_location
                startsLocation: model.starts_location
                reactions: model.reactions
                onOpenRequested: root.openRequested(fileUrl, fileName, viewType,
                                                    previewWidth, model.message_id)
                onFullTextRequested: root.fullTextRequested(model.message_id,
                                                           model.sender_name)
                onAppRequested: root.appRequested(model.message_id)
                onCallRequested: root.callRequested(model.message_id,
                                                    model.is_outgoing,
                                                    model.call_state,
                                                    model.call_has_video)
                onDownloadRequested: root.downloadRequested(model.message_id)
                onReactionRequested: root.reactionRequested(model.message_id, emoji)
                // Read on tap, not bound: rows without role (tests' `ListModel`) stay quiet.
                onQuoteRequested: root.quoteRequested(model.quote_message_id || 0)
                onLocationRequested: root.locationRequested(model.message_id, model.sender_id,
                                                            model.sender_name)
                // Long press on chip, download or play: row menu.
                onMenuRequested: messageRow.openMenu()
            }
        }
    }

    /// Fetched yet; else each open flashed "no messages yet".
    property bool loaded: true

    ViewPlaceholder {
        objectName: "emptyPlaceholder"
        enabled: root.loaded && root.count === 0
        text: root.placeholderText
    }
}
