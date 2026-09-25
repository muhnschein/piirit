import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The messages of one conversation. Its own component so the scrolling can
 * be driven in a test -- ConversationPage cannot be loaded headlessly.
 *
 * Opens on the newest message and follows arrivals, but only while the
 * reader is already down there: scrolling away from history someone is
 * reading is worse than missing an arrival.
 */
SilicaListView {
    id: root

    // Groups have to say who is speaking; one-to-one chats do not.
    property bool showSender: false

    /// The message the "new messages" line is drawn above, 0 for none.
    /// The model reads it from the core when the chat is opened and does
    /// not move it afterwards; see `ChatMessages.unread_from`.
    property int unreadFrom: 0
    property string placeholderText
    /// How a message body is drawn: 0 Markdown, anything else as written.
    /// The page binds it from the reader's setting.
    property int markdownMode: 1
    /// Whether a message of the reader's own is offered for editing at
    /// all: the chat takes messages, and is encrypted. The page binds it
    /// from the model, which asks the core; what the row adds is whether
    /// this message is one the core would let them edit.
    property bool canEdit: false
    /// Whether webxdc apps are on. Handed down to each row, which draws a
    /// `.xdc` as a file rather than an app without it. The page binds it
    /// from the reader's setting, as it does the Markdown mode.
    property bool appsEnabled: false

    property bool stickToBottom: true

    /// The view has moved; these rows want filling in.
    ///
    /// The model holds a row for every message in the chat and fills in
    /// only the ones somebody is looking at, so this is what asks. Raised
    /// rather than acted on, like every other request here: the component
    /// knows nothing about the core.
    signal hydrateRequested(int first, int last)

    /// Ask for whatever is on screen to be filled in.
    ///
    /// `indexAt` lands on nothing between rows and over a day separator, so
    /// this walks the view rather than probing its two edges. Debounced by
    /// the timer below: a flick would otherwise ask once per frame.
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

    // Short: this is the wait between the reader arriving somewhere and the
    // rows there having anything in them, and the system's scroll-to-top
    // arrives in one step rather than over a flick's worth of frames. Long
    // enough still to coalesce a flick, which changes `contentY` every frame
    // and would otherwise walk the view for each of them.
    Timer {
        id: fillRows
        interval: 60
        onTriggered: root.askForRows()
    }

    /// Ask for rows a moment from now, unless an ask is already on its way.
    ///
    /// Started, never restarted. A flick changes `contentY` every frame, so
    /// a timer restarted on each of them would not fire until the flick had
    /// stopped, and a fast scroll into the history would end on a screen of
    /// blanks. Left to run, it fires every sixty milliseconds of a flick
    /// instead, and the rows in front of the reader are asked for while
    /// they are still on their way there.
    function askSoon() {
        if (!fillRows.running) {
            fillRows.start()
        }
    }

    // How many rows the model holds. Bound by the page rather than read off
    // the view: `count` there only changes when the view has laid out, and
    // an arrival has to be noticed whether or not it is on screen yet.
    property int messageCount: 0
    // How many have arrived since the reader scrolled away. Counted from
    // what the model says arrived, not by differencing `messageCount`: a
    // deletion moves that too, and a removal landing with an arrival in one
    // reload does not move it at all.
    property int missedCount: 0

    /// Messages from other people have just been added.
    function noteArrivals(count) {
        if (!root.following) {
            root.missedCount += count
        }
    }

    // Raised rather than acted on: the component knows nothing about the
    // core, which is what makes it loadable on its own.
    signal replyRequested(int messageId, string body, string author)
    /// The reader asked to change the text of a message of their own.
    /// The text travels with it: the field is filled from here, and this
    /// row may be gone by the time the page gets round to it.
    signal editRequested(int messageId, string body)
    signal copyRequested(string body)
    /// A message's wait is up, and it goes now -- from this account's
    /// devices alone, or from everybody's, which is what the reader
    /// picked when they asked.
    signal deleteRequested(int messageId, bool forEveryone)
    /// The reader asked to delete a message and has not said which kind
    /// of delete yet. Which is a dialog of its own
    /// (pages/DeleteMessageDialog.qml), and pushing one is the page's
    /// business, not this component's. `canDeleteForEveryone` travels
    /// with the ask because only the row knows it, and the row may be
    /// gone by the time the answer comes back.
    signal deleteChoiceRequested(int messageId, bool canDeleteForEveryone)
    signal resendRequested(int messageId)
    signal forwardRequested(int messageId)
    /// The reader tapped an attachment. What opening it means -- a page
    /// here, or handing it to another app -- is the page's decision.
    signal openRequested(url fileUrl, string fileName, string viewType,
                         real previewWidth)
    /// The reader asked to keep a copy of an attachment somewhere they
    /// can find it again, which is the page's to decide. The name
    /// travels with it: the core keeps the file under a name of its own,
    /// and the copy is to be under the sender's.
    signal saveRequested(url fileUrl, string fileName)
    /// The reader asked to read one message on a page of its own. The
    /// author travels with it: the page names who wrote what it shows,
    /// and this row may be gone by the time it is built.
    signal fullTextRequested(int messageId, string author)
    /// The reader asked for the rest of a message the download limit
    /// held back.
    signal downloadRequested(int messageId)
    /// The reader tapped a webxdc app. Running one is a page, which is
    /// the page's to push.
    signal appRequested(int messageId)
    /// The reader tapped a call. Which call, and where it stood, so the
    /// page can tell one still ringing from one to call back.
    signal callRequested(int messageId, bool outgoing, string callState)
    /// The reader picked an emoji for a message, from the menu or from a
    /// chip already on it. Whether that puts it on or takes it off is the
    /// model's to decide, from what it knows the reader already sent.
    signal reactionRequested(int messageId, string emoji)

    /// How long a message waits before it goes, in milliseconds. The
    /// page does not set it; a test turns it down rather than waiting.
    property alias pendingDelay: doomedMessages.delay

    /// Send everything still waiting, now. The page calls this on its
    /// way out of the chat (ConversationPage).
    function flushDeletes() {
        doomedMessages.flush()
    }

    /// Whether this message is waiting to go. For a test to read: what
    /// each row does with the answer is its own binding.
    function pendingFor(messageId) {
        return doomedMessages.pending(messageId)
    }

    /// The reader picked one of the two deletes, on the page the menu
    /// led to. The wait starts now, carrying which kind it is, and the
    /// row puts the countdown up off `doomed` -- the row the menu was
    /// opened on may well have been rebuilt while the page was up.
    function confirmDelete(messageId, forEveryone) {
        doomedMessages.ask(messageId, forEveryone === true)
    }

    /// The messages the reader has asked to delete, waiting out the
    /// moment in which they can say they did not mean it.
    ///
    /// Not Silica's `remorseAction`, which would put the wait on the row
    /// -- and deleting a message is exactly what destroys rows, so a run
    /// of deletes lost all but the first. See PendingRemoval.
    PendingRemoval {
        id: doomedMessages
        // `tag` is what the reader picked on the page: true for a delete
        // that asks the other ends to delete their copies too.
        onRemove: root.deleteRequested(id, tag === true)
    }

    /// The emoji the menu offers first, as the reference clients offer
    /// them. Anything else is a chip someone else's reaction has put on
    /// the message, which a tap answers in kind.
    readonly property var quickReactions: ["👍", "❤️", "😂", "😮", "😢", "🙏"]

    /// Whether a tap on an attachment of this kind opens a page of the
    /// app's own -- PicturePage for a picture, VideoPage for a video --
    /// rather than handing the file to the system. Those pages carry
    /// Open and Save on their pull-down, so the row's menu does not
    /// offer them a second time. What the page pushes for each kind is
    /// ConversationPage.openAttachment.
    function opensOnItsOwnPage(viewType) {
        return viewType === "Image" || viewType === "Gif"
               || viewType === "Sticker" || viewType === "Video"
    }

    /// Back to the newest message, and following again.
    function jumpToNewest() {
        // The button sits over the list rather than inside it, so a tap
        // does not stop an inertial flick the way touching the list would.
        // Left running, `held` stays true, `following` stays false, and the
        // scroll below never happens -- while the state this sets has
        // already told the page the reader is at the newest message.
        root.cancelFlick()
        root.held = false
        // A held row is let go of as well. Coming back from a picture
        // holds the row the reader left on for a while, and a hold is
        // re-applied on every change to the content -- so a jump made
        // inside that while went to the end and was put straight back on
        // the next row measured, with the button already gone.
        root.releaseRow()
        root.stickToBottom = true
        root.missedCount = 0
        toEnd.restart()
        // Last, because the jump is the part that has to happen. A tap
        // on the button does not reach the row a context menu is open on
        // either, so Silica leaves the menu up -- and while one is up
        // nothing here moves the view, which left the button doing
        // nothing at all. The reader is leaving the row the menu is on,
        // so the menu goes with them; the scroll above is already asked
        // for by the time it does.
        root.closeOpenMenu()
    }

    // A list draws its delegates outside its own box unless told not to,
    // and what sits below this one is translucent.
    clip: true

    // Never straight away: a model handed in at construction has already
    // filled, and rows still have to be measured, so where the end is is
    // not known until the pass after whatever prompted this.
    Timer {
        id: toEnd
        interval: 0
        // Checked again here: the reader can scroll away between the
        // arrival that started this and the pass it fires on.
        onTriggered: {
            if (root.following && !root.menuOpen) {
                root.positionViewAtEnd()
                root.reachedEnd = true
            }
        }
    }

    /// Whether the view has ever actually been put at the newest message.
    ///
    /// Only true after that, because until then a view sitting far from the
    /// end is a chat that has not opened yet rather than a reader who has
    /// gone somewhere.
    property bool reachedEnd: false

    // True between the start and end of a drag or flick. Tracked rather
    // than read off `moving`, which no test can set.
    property bool held: false

    // Not while the reader has hold of it. Rows are measured as they come
    // into view, so a drag upwards grows `contentHeight` on its own, and
    // following that hauls them straight back down again.
    readonly property bool following: root.stickToBottom && !root.held

    /// The row whose context menu is open, null for none.
    ///
    /// A long press is not a drag -- `onMovementStarted` never fires for
    /// one -- so nothing here knew a menu had been opened, while
    /// unfolding one is the largest change a row's height ever makes and
    /// Silica scrolls the view itself to bring the whole menu on screen.
    /// Every move made from here during that unfold is a move fighting
    /// that one: the follow threw the view at the newest message the
    /// moment a menu finished opening, and a hold put a menu opened on a
    /// held row straight back under the bottom of the view. So the view
    /// is left alone while a menu is up, and this is what says so.
    ///
    /// Typed as an Item so that a row destroyed with its menu open --
    /// scrolled out of view, or taken by a reload -- clears this by
    /// itself rather than leaving the view pinned for good.
    property Item openMenuRow: null
    readonly property bool menuOpen: root.openMenuRow !== null

    /// A row's context menu has opened.
    ///
    /// The reader is looking at the menu, so a held row is let go of
    /// here exactly as it is when a drag takes the view over: what the
    /// hold is for is putting the reader back after the content moves
    /// under them, and this time the content moved because they asked
    /// it to.
    function menuOpened(row) {
        root.openMenuRow = row
        root.releaseRow()
    }

    /// Take down whatever menu is open, and stop waiting on it.
    ///
    /// Forgotten here before Silica is asked, so that the row reporting
    /// itself closed a moment later has nothing of its own to answer for
    /// and leaves whatever asked for this to finish its own work.
    function closeOpenMenu() {
        var row = root.openMenuRow
        if (!row) {
            return
        }
        root.openMenuRow = null
        row.closeMenu()
    }

    /// A row's context menu has closed.
    function menuClosed(row) {
        // Only the row that opened it. A menu opening on a second row
        // closes the first, and the two arrive in no fixed order.
        if (root.openMenuRow !== row) {
            return
        }
        root.openMenuRow = null
        // The row shrinks back as the menu folds away, and a view that
        // was following the newest message goes back to it: the fold is
        // the last thing to change the content, so without this the
        // reader is left a menu's height short of the end.
        if (root.following) {
            toEnd.restart()
        }
    }

    // Taking hold of the list is taking it over, so a held row lets go.
    onMovementStarted: {
        root.held = true
        root.releaseRow()
    }

    // Both of these, because a row arriving and that row being measured are
    // separate steps: the first moves `count`, the second `contentHeight`.
    onMessageCountChanged: if (root.following && !root.menuOpen) toEnd.restart()
    onContentHeightChanged: {
        // A menu unfolding is the content growing a row at a time, and
        // the reader is looking straight at it. Neither the hold nor the
        // follow gets a say until it is closed again: Silica is moving
        // the view to bring the menu on screen, and this is the handler
        // that was moving it back.
        if (root.menuOpen) {
            root.askSoon()
            return
        }
        // A held row first: the content changing height is exactly what
        // moves the reader off it, so this is the moment to put them back.
        if (root.pendingRow >= 0) {
            root.putBack()
        } else if (root.following) {
            toEnd.restart()
        }
        // And whatever is on screen now wants filling in. Opening a chat
        // may never move contentY at all, so this is the ask that covers
        // the first screen.
        root.askSoon()
    }

    // Where the view was before this change, so a move can be told from
    // the content growing under a view that has not moved.
    property real lastContentY: 0

    onContentYChanged: {
        var movedUp = root.contentY < root.lastContentY
        root.lastContentY = root.contentY
        // While the page is away nobody is scrolling, so anything moving
        // the view is the list losing its place rather than the reader
        // choosing to: put it back in the same turn, before a frame of
        // the wrong place is drawn.
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
        // Scrolling through rows that are all still placeholders does not
        // change `contentHeight` at all -- they are the same height as each
        // other -- so without this the reader can walk into a screenful of
        // blanks and nothing ever asks for them.
        root.askSoon()
        // Something has moved the view up, a long way from the newest
        // message, without touching it: the system's own scroll-to-top,
        // which is how one gets to the beginning of a chat. Following would
        // notice the next row being measured and haul the reader straight
        // back down, which is being thrown to the newest message a moment
        // after asking for the oldest. A drag or a flick is excluded by
        // `held`, a jump this component made by `pendingRow`, and the chat
        // still opening by `reachedEnd`.
        //
        // Only a move *up*. The view moves down on its own for the other
        // reason as well: the last row growing as its picture decodes, or
        // the view clamping to an end that has just moved. Neither is the
        // reader leaving, and taking them for it was how a tall picture at
        // the end of a chat came to be shown from its top -- the view was
        // put at the end of the row as it stood, the row grew, and the
        // move that would have followed it down had been switched off by
        // the one before.
        //
        // Nor is the scroll Silica makes to bring a context menu on
        // screen, which moves the view up whenever the menu it is
        // revealing sits below the bottom of it -- and dropping the
        // follow there left a reader who had opened a menu at the newest
        // message no longer at it once they closed it again.
        if (movedUp && root.reachedEnd && root.stickToBottom && !root.held
                && !root.restoring && root.pendingRow < 0 && !root.nearBottom
                && !root.menuOpen) {
            root.stickToBottom = false
        }
    }

    /// The message a search sent the reader here for, flashed once so it
    /// can be picked out of the wall of text around it. 0 for none.
    property int foundMessageId: 0

    /// Put a row in the middle of the view and stay there.
    ///
    /// Opening a chat at its newest message when the reader asked for one
    /// from last March is the difference between finding something and
    /// being told roughly where it is. `stickToBottom` goes off first:
    /// otherwise the next arrival drags the view back to the bottom and
    /// away from what they came to read.
    function jumpToRow(index) {
        if (index < 0) {
            return
        }
        root.stickToBottom = false
        root.positionViewAtIndex(index, ListView.Center)
    }

    /// Put the view on a row and keep it there while the rows settle.
    ///
    /// One `positionViewAtIndex` is enough only where every row is already
    /// its final height, which is nowhere real. Rows are measured as they
    /// are laid out, wrapped text at the device's own metrics is taller
    /// than an estimate, a picture's row changes height again when the
    /// picture decodes, and the header above the oldest row collapses to
    /// nothing the moment there is no more history to offer. Every one of
    /// those that happens above the reader moves them, and they all happen
    /// after the jump.
    ///
    /// So the row is held rather than jumped to: re-applied on each change
    /// to the content until the reader takes the view over or it stops
    /// moving under them. This is what a search result lands on and what
    /// the beginning of the chat lands on, and both were reported landing
    /// at the top of whatever had just loaded instead.
    function holdAt(index) {
        root.hold(index, false, 0)
    }

    /// Keep a row where it is: its top `offset` below the top of the view,
    /// rather than in the centre.
    ///
    /// What coming back to the page holds. A reader stops wherever their
    /// thumb leaves the list, which is never a row's centre, so holding
    /// the row they left on *at* the centre moved the view by however far
    /// it was from there -- the small jump, up or down by the row, at the
    /// end of every swipe back, once the rest of the return was smooth
    /// enough for it to show.
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

    /// Let go of the held row: the reader has taken over, or the view has
    /// stopped moving under them.
    function releaseRow() {
        holdDeadline.stop()
        root.away = false
        root.pendingRow = -1
    }

    /// The row a jump is holding the view on, or -1.
    property int pendingRow: -1
    /// Where it is held: its top `pendingOffset` below the top of the view
    /// when `pendingPlaced`, in the centre of the view otherwise.
    property bool pendingPlaced: false
    property real pendingOffset: 0
    /// True while `putBack` is running, so the view moving because this
    /// moved it does not read as one more reason to move it.
    property bool restoring: false

    function putBack() {
        // Not under an open menu, whoever asks. A hold is let go of when
        // one opens, but the page can arm a fresh one at any time -- a
        // search result landing, a step back through the history -- and
        // a hold applied over an open menu drags it off the screen the
        // reader just opened it on.
        if (root.restoring || root.menuOpen || root.pendingRow < 0) {
            return
        }
        root.restoring = true
        if (root.pendingPlaced) {
            root.placeRow(root.pendingRow, root.pendingOffset)
        } else {
            // The first row has nothing above it to be centred against,
            // and asking for its centre relies on the view clamping.
            // Beginning is what "the top" means.
            root.positionViewAtIndex(
                root.pendingRow,
                root.pendingRow === 0 ? ListView.Beginning : ListView.Center)
        }
        root.restoring = false
    }

    /// Put row `index` with its top `offset` below the top of the view.
    ///
    /// Nothing moves if that is where it already is. `positionViewAtIndex`
    /// cannot promise that -- it knows the top, the centre and the end of
    /// the view, none of which is where the reader stopped -- so the row
    /// is looked for on screen first, brought on only if it is not there,
    /// and the view then moved by exactly the difference, which is nothing
    /// for a view nothing has moved.
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

    /// The item drawing row `index`, if the row is on screen.
    ///
    /// Walked the way `askForRows` walks the view: `itemAt` answers only
    /// for a point inside a row, and lands on nothing between two.
    function itemOf(index) {
        for (var y = 0; y < root.height; y += Theme.paddingLarge) {
            if (root.indexAt(root.width / 2, root.contentY + y) === index) {
                return root.itemAt(root.width / 2, root.contentY + y)
            }
        }
        return undefined
    }

    /// `y`, kept between the ends of the content as a drag would keep it.
    function withinContent(y) {
        var top = root.originY
        var bottom = Math.max(top, root.contentHeight + root.originY - root.height)
        return Math.min(Math.max(y, top), bottom)
    }

    // A held row is let go of when the reader takes the view over, and
    // otherwise not until this. No shorter timer sized to the movement
    // itself: a device does not move its content in one run -- it lays the
    // rows out, goes quiet while a picture decodes, and moves them again --
    // so such a timer expires in the gap and the hold is gone by the time
    // the reader is carried off. Holding a view nobody is touching costs
    // nothing.
    Timer {
        id: holdDeadline
        interval: 6000
        onTriggered: root.releaseRow()
    }

    /// Where the reader is, before another page goes over this one.
    ///
    /// A conversation with a picture opened over it once came back with
    /// its view at the top of the loaded messages: every row collapsed
    /// under the covering page and the list lost its place. The rows keep
    /// their height under a page now, but a list that loses its place for
    /// any other reason -- a reload, a row gone -- still comes back here.
    /// Remembered as a row and where its top was, from the top of the
    /// view: a row rather than a pixel, for the same reason a step back
    /// through the history is, and the offset so that going back to it
    /// is going back to the pixel.
    property int rememberedRow: -1
    property real rememberedOffset: 0
    property bool rememberedFollowing: false
    /// True between the page going away and coming back, during which the
    /// row is held with no deadline: a reader can look at a picture for as
    /// long as they like.
    property bool away: false

    function rememberPlace() {
        root.rememberedFollowing = root.stickToBottom
        root.rememberedRow = -1
        root.rememberedOffset = 0
        // The first row at or below the middle of the view. `indexAt`
        // lands on nothing between rows or over a day separator, so a
        // single probe answers -1 about half the time.
        for (var y = root.height / 2; y < root.height; y += Theme.paddingLarge) {
            var index = root.indexAt(root.width / 2, root.contentY + y)
            if (index >= 0) {
                root.rememberedRow = index
                root.rememberedOffset =
                    root.itemAt(root.width / 2, root.contentY + y).y - root.contentY
                break
            }
        }
        // Armed, not merely written down. Putting the view back when the
        // page returns is too late: a list that loses its place while the
        // page is away paints a frame of the top of the chat before
        // anything gets round to correcting it -- the flash of the oldest
        // messages, followed by being yanked back. Armed, the loss is
        // undone in the same turn it happens and no wrong frame is ever
        // drawn. A view that was following is armed too, and goes back
        // to the end rather than to a row; see `onContentYChanged`.
        root.away = true
        if (root.rememberedRow >= 0 && !root.rememberedFollowing) {
            root.pendingRow = root.rememberedRow
            root.pendingPlaced = true
            root.pendingOffset = root.rememberedOffset
            holdDeadline.stop()
        }
    }

    function restorePlace() {
        // The hold that was armed on the way out ends here, whatever
        // happens next: from now on the reader is looking at this page and
        // the deadline applies again.
        root.away = false
        if (root.rememberedFollowing) {
            // Nothing to put back. A view following the newest message
            // was kept at the end while away -- by the arming above for a
            // list that lost its place, by `toEnd` for each arrival --
            // and sending it to the end once more moved a reader who had
            // stopped a line short of it, following but not at the very
            // end, on every return.
            root.releaseRow()
        } else if (root.rememberedRow >= 0) {
            // Held again rather than merely positioned: coming back is a
            // relayout like any other, and the rows settle after it.
            root.holdPlace(root.rememberedRow, root.rememberedOffset)
        } else {
            root.releaseRow()
        }
        root.rememberedRow = -1
    }

    // How close to the end still counts as being at it. Exactly `atYEnd`
    // is the wrong test: rows are measured as they scroll into view, so
    // `contentHeight` is still growing at the moment a scroll stops, and
    // the comparison lands just short. That is why the button could not
    // be dismissed -- `jumpToNewest` set `stickToBottom`, the scroll it
    // started then ended, `onMovementEnded` read `atYEnd` as false and put
    // it straight back. A reader a line short of the bottom has, for every
    // purpose this drives, arrived.
    readonly property real bottomSlack: Theme.itemSizeLarge

    /// At or near the newest message -- including a chat too short to
    /// scroll at all, where there is no "end" to reach.
    readonly property bool nearBottom:
        root.contentHeight <= root.height
        || root.contentY + root.height
           >= root.contentHeight + root.originY - root.bottomSlack

    // Where the reader left off, once they stop moving.
    onMovementEnded: {
        root.held = false
        root.stickToBottom = root.nearBottom
        if (root.nearBottom) {
            root.missedCount = 0
        }
    }

    // Arriving at the newest message, by scrolling or by the button, is
    // what counts as having read what is there.
    onStickToBottomChanged: if (root.stickToBottom) root.arrivedAtNewest()
    signal arrivedAtNewest()


    // The model counts days in the viewer's timezone, so grouping by that
    // number is enough to break the list into days.
    //
    // Declared without a `section.delegate`: the grouping is wanted, the
    // separate item is not. A section delegate is its own item, positioned
    // by the view above the row it heads and sized from whatever height it
    // reported when the view last measured it -- and a heading drawn over
    // the message beneath it was the report. Getting the height right at
    // creation was not enough, and the bookkeeping that decides where the
    // row goes is the view's rather than ours.
    //
    // So the heading is drawn *inside* the row instead, in `dayHeading`
    // below. `ListView.section` and `ListView.previousSection` come from
    // this property alone -- the view fills them in whether or not there is
    // a delegate to build -- so the view still says where a day starts, and
    // the heading is part of the row's own height. A row cannot be drawn
    // over itself.
    section.property: "day_number"

    delegate: ListItem {
        id: messageRow
        objectName: "messageRow"

        // The view has to know, because it spends the rest of its time
        // moving itself: following the newest message, putting a held
        // row back. A menu unfolding does both of the things those watch
        // for -- the content grows, and Silica scrolls to bring the menu
        // on screen -- and a row is the only thing that can say which of
        // them is happening.
        onMenuOpenChanged: {
            if (messageRow.menuOpen) {
                root.menuOpened(messageRow)
            } else {
                root.menuClosed(messageRow)
            }
        }

        // A Component rather than a menu built with the row. Silica builds
        // a Component the first time the menu is opened; a ContextMenu
        // declared here outright was built with every row, and it is the
        // biggest thing on one -- six reactions and eight items, thirty
        // objects, for a long press most rows never get. What a row costs
        // to build is what a flick costs per frame, and what coming back
        // to a conversation costs while the page is still sliding in.
        menu: Component {
            ContextMenu {
                id: rowMenu

                // The quick reactions, above the actions: one tap on an emoji
                // and the menu is done. Not a MenuItem, which is one line of
                // text; the menu takes any item, and lays this one out like
                // the rest.
                Item {
                    id: reactionPicker
                    objectName: "reactionPicker"
                    // A core notice is nobody's message to react to.
                    readonly property bool shown: !model.is_info
                    visible: reactionPicker.shown
                    width: parent ? parent.width : 0
                    height: reactionPicker.shown ? Theme.itemSizeSmall : 0
                    /// Taken while the row is here, like Delete's id: the
                    /// menu can outlive the row it was opened on.
                    readonly property int messageId: model.message_id

                    Row {
                        anchors.centerIn: parent
                        spacing: Theme.paddingMedium

                        Repeater {
                            model: root.quickReactions

                            MouseArea {
                                objectName: "reactionOption"
                                width: Theme.itemSizeSmall
                                height: Theme.itemSizeSmall
                                readonly property string emoji: modelData
                                function choose() {
                                    root.reactionRequested(reactionPicker.messageId, emoji)
                                    rowMenu.close()
                                }
                                onClicked: choose()

                                Label {
                                    anchors.centerIn: parent
                                    font.pixelSize: Theme.fontSizeLarge
                                    textFormat: Text.PlainText
                                    text: modelData
                                }
                            }
                        }
                    }
                }

                MenuItem {
                    objectName: "replyItem"
                    // A core notice is nobody's message to answer.
                    visible: !model.is_info
                    text: qsTr("Reply")
                    onClicked: root.replyRequested(model.message_id, model.text,
                                                   model.sender_name)
                }
                MenuItem {
                    objectName: "editItem"
                    // Only what the core will take an edit of, which is
                    // what deltachat-android asks before offering it: a
                    // message of one's own, not a notice, not a call,
                    // with text to change -- an edit cannot add words to
                    // a picture sent bare -- and not one the sending
                    // core cut, whose whole text is not here to edit.
                    visible: root.canEdit && model.is_outgoing && !model.is_info
                             && model.text.length > 0 && !model.has_html
                             && model.view_type !== "Call"
                    text: qsTr("Edit")
                    onClicked: root.editRequested(model.message_id, model.text)
                }
                MenuItem {
                    objectName: "copyItem"
                    // An image or a voice message with no caption has no text:
                    // copying one emptied the clipboard and said it had worked.
                    visible: model.text.length > 0
                    text: qsTr("Copy")
                    onClicked: root.copyRequested(model.text)
                }
                MenuItem {
                    objectName: "openItem"
                    // Only a message that carries one; a webxdc app is run
                    // rather than opened, and has its own tap. With apps off
                    // there is nothing to run, and the .xdc is a file like
                    // any other -- which is what the row already draws. A
                    // picture or a video has a page of its own, whose
                    // pull-down offers this, so the menu does not.
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
                    // The reader's own copy, outside the app: what makes a
                    // file somebody sent theirs rather than the chat's.
                    // Not for what has a page of its own, for the reason
                    // Open is not.
                    visible: model.file_path.length > 0
                             && !root.opensOnItsOwnPage(model.view_type)
                    text: qsTr("Save")
                    onClicked: root.saveRequested("file://" + model.file_path,
                                                  model.file_name)
                }
                MenuItem {
                    objectName: "forwardItem"
                    // A core notice is not the reader's to pass on.
                    visible: !model.is_info
                    text: qsTr("Forward")
                    // Taken now rather than in the callback: picking a chat
                    // takes a page push, and this row may be gone by the time
                    // the answer comes back -- the same reason Delete hoists
                    // its id.
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
                    // The two states the rest of a message can be asked
                    // for in; the row offers the same tap.
                    visible: model.download_state === "Available"
                             || model.download_state === "Failure"
                    text: qsTr("Download")
                    onClicked: root.downloadRequested(model.message_id)
                }
                MenuItem {
                    objectName: "deleteItem"
                    text: qsTr("Delete")
                    // One entry, not two: which kind of delete is asked
                    // in a dialog (pages/DeleteMessageDialog.qml),
                    // because "for me" and "for everyone" side by side on
                    // a long-press menu is how the wrong one gets tapped.
                    //
                    // Both are read now rather than in the answer:
                    // choosing takes as long as the reader takes, and
                    // this row may be gone by then -- the same reason
                    // Forward hoists its id. What the core will take a
                    // deletion for everyone of is a message this account
                    // sent, and an encrypted one; a notice is nobody's
                    // message at all.
                    onClicked: root.deleteChoiceRequested(
                                   model.message_id,
                                   model.is_outgoing && model.show_padlock
                                   && !model.is_info)
                }
            }
        }
        // Sized by its content, not fixed: a device message runs to a
        // dozen wrapped lines, and a fixed row height makes them overlap
        // each other and the header. A row whose message has not been
        // fetched yet stands at one line, which is what gives the list a
        // length before any of it has been read. The day heading, when
        // this row carries one, is part of that height rather than
        // something the view has to find room for.
        contentHeight: dayHeading.height + unreadLine.height
                       + (model.loaded ? body.height : Theme.itemSizeExtraSmall)

        /// This message is on its way out.
        readonly property bool doomed: doomedMessages.pending(model.message_id)

        /// Whether this row has already put a countdown up over the wait
        /// it is in. The wait is asked for on a page rather than in the
        /// menu now, so the row that shows it is whichever row is here
        /// when the answer lands -- and both the answer and a rebuild
        /// raise it. Two `execute` calls over one item is a countdown
        /// restarting itself under the reader.
        property bool countingDown: false

        // The wait starts elsewhere: on the page the menu leads to, or
        // in the list while this row was scrolled away. Either way the
        // row learns of it here and puts the platform's countdown up.
        onDoomedChanged: {
            if (messageRow.doomed) {
                messageRow.raiseRemorse()
            } else {
                messageRow.countingDown = false
            }
        }

        /// Silica's own countdown, drawn over the message: the bar, the
        /// seconds, "Tap to cancel", all of it the platform's.
        ///
        /// The *deletion* is not its business -- that belongs to
        /// `doomedMessages`, because a remorse item lives in the row it
        /// covers and a row is what a delete destroys. So it is handed a
        /// callback that does nothing and asked only to draw and to
        /// report the tap.
        function raiseRemorse() {
            if (messageRow.countingDown) {
                return
            }
            messageRow.countingDown = true
            remorse.active = true
            //: What Silica's countdown says it is doing, over a
            //: message the reader has asked to delete.
            remorse.item.execute(
                body, qsTr("Deleting"), function() {},
                doomedMessages.countdownFor(model.message_id))
        }

        // Built the first time a delete is asked for, not with the row: the
        // platform's countdown is a dozen items of its own, and every row
        // carried one for a tap that almost never comes.
        Loader {
            id: remorse
            active: false
            sourceComponent: RemorseItem {
                objectName: "messageRemorse"
                onCanceled: doomedMessages.spare(model.message_id)
            }
        }

        // A row is rebuilt every time it scrolls back into view, so one
        // scrolled past mid-wait comes back with no countdown on it. Put
        // it up again with what is actually left of the wait.
        Component.onCompleted: {
            if (messageRow.doomed) {
                messageRow.raiseRemorse()
            }
        }

        // One surface: a tap opens whatever the message has to open, a
        // long press opens the menu, wherever on the row either lands.
        // The row is what takes the press, so the two cannot fight. A
        // message waiting to go is covered by the remorse, which takes
        // the tap itself and calls the delete off.
        onClicked: body.tapped()

        /// The date this row's day starts under, on the first row of each
        /// day and nowhere else.
        ///
        /// Inside the row rather than a `section.delegate`, so that where it
        /// sits is arithmetic here rather than the view's bookkeeping: see
        /// `section.property` above.
        Label {
            id: dayHeading
            objectName: "dayLabel"
            width: parent.width
            // The view fills these in from `section.property`. A row whose
            // day differs from the one before it is the first of its day;
            // the first row in the list has no previous section, which
            // reads as an empty string and so counts as a change.
            //
            // Day 0 is a row whose day is not known, which happens only if
            // the core answers the id list without day markers. Heading a
            // run of those with the epoch would be worse than heading them
            // with nothing.
            // Attached to the delegate root, not to this label: `ListView`
            // attaches to the item the view created. Outside a view both
            // read undefined, which is not "0" and does equal itself, so
            // this comes out false rather than erroring.
            //
            // Sized by that reason and not by `visible`, which is the
            // effective one and goes false for the whole page while
            // another is over it: see MessageDelegate.
            readonly property bool shown: messageRow.ListView.section !== "0"
                                          && messageRow.ListView.section
                                             !== messageRow.ListView.previousSection
            visible: dayHeading.shown
            height: dayHeading.shown ? implicitHeight + Theme.paddingMedium : 0
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            // No offset arithmetic: the section is a day number, so read
            // its calendar date in UTC and rebuild it as a local date with
            // the same parts. Subtracting an offset here would reintroduce
            // exactly the daylight-saving error the model now avoids.
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

        /// Where the reader left off: everything below this line arrived
        /// while they were away.
        ///
        /// Inside the row for the reason the day heading is -- see
        /// `section.property` above -- and under the day heading when a
        /// row carries both, so the order reads "Tuesday, and here is
        /// what is new".
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

        // Only what is on screen is built, so this is not the whole chat's
        // worth of delegates -- but a placeholder must not try to draw a
        // message it has not got.
        MessageDelegate {
            id: body
            objectName: "messageDelegate"
            visible: model.loaded
            // Neither hidden nor faded here while the message waits to
            // go: the remorse covering it does the fading, with its own
            // `opacity: 0.0` on what it was handed. Hiding it would be
            // wrong anyway -- that takes its children's `visible` with
            // it, and every part of a message measures
            // `visible ? implicitHeight : 0`, so the row would collapse
            // under the countdown drawn over it.
            enabled: !messageRow.doomed
            y: dayHeading.height + unreadLine.height
            width: parent.width
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
                                                model.call_state)
            onDownloadRequested: root.downloadRequested(model.message_id)
            onReactionRequested: root.reactionRequested(model.message_id, emoji)
            // A long press on a chip, the download offer or a play
            // button: those take the press for themselves, and hand the
            // long one back to the row it means.
            onMenuRequested: messageRow.openMenu()
        }
    }

    /// Whether the chat's messages have actually been fetched yet.
    ///
    /// Not the same as having none. Without it, every open flashed "no
    /// messages yet" while the history was still on its way.
    property bool loaded: true

    ViewPlaceholder {
        objectName: "emptyPlaceholder"
        enabled: root.loaded && root.count === 0
        text: root.placeholderText
    }
}
