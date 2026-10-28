//! Where conversation sits on return; when new message may move it.

// Qt harness: `unsafe` env::set_var pre-Qt (`unused_unsafe`: unsafe only from edition 2024),
// `borrow_as_ptr` for engine pointer.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

/// List over plain `ListModel` with real roles.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 400

        ListModel { id: rows }

        Loader {
            id: loader
            anchors.fill: parent
        }

        function append(count) {
            for (var i = 0; i < count; i++) {
                rows.append({
                    // 1-based and distinct: with every id 0 an assertion on
                    // one cannot tell a right answer from a missing role.
                    message_id: rows.count + 1,
                    text: 'message number ' + rows.count + ', long enough '
                          + 'to take a line of its own in the list',
                    is_outgoing: false, is_info: false, show_padlock: true,
                    state: 16, timestamp: 1700000000 + rows.count,
                    day_number: 19675, sender_name: 'Ada',
                    sender_color: '#00875a', quote_text: '', quote_author: '',
                    file_path: '', file_name: '', view_type: 'Text',
                    image_width: 0, image_height: 0
                })
            }
            // As the page does: the model says what arrived, the view
            // counts it. Differencing the row count instead would count a
            // message the reader sent, and miss an arrival that landed in
            // the same reload as a deletion.
            if (loader.item) { loader.item.noteArrivals(count) }
            return '' + rows.count
        }

        // A row the reader sent. It moves the count without being an
        // arrival, so nothing tells the view about it.
        function appendOwn() {
            rows.append({
                message_id: rows.count + 1, text: 'mine',
                is_outgoing: true, is_info: false, show_padlock: true,
                state: 26, timestamp: 1700000000 + rows.count,
                day_number: 19675, sender_name: 'Me',
                sender_color: '#00875a', quote_text: '', quote_author: '',
                file_path: '', file_name: '', view_type: 'Text',
                image_width: 0, image_height: 0
            })
            return '' + rows.count
        }

        property string raised: ''
        property int arrivals: 0

        function load(url) {
            loader.setSource(url, { model: rows, title: 'Ada' })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            var view = loader.item
            // As the page binds it.
            view.messageCount = Qt.binding(function() { return rows.count })
            view.replyRequested.connect(function(id, body, author) {
                raised = 'reply:' + id + ':' + body + ':' + author
            })
            view.copyRequested.connect(function(body) { raised = 'copy:' + body })
            // Delete asks before it deletes: the menu raises the ask, the
            // page behind it says which kind, and only then does the wait
            // start and this arrive.
            view.deleteRequested.connect(function(id, forEveryone) {
                raised = 'delete:' + id + ':' + forEveryone
            })
            view.deleteChoiceRequested.connect(function(id, canForAll) {
                raised = 'ask:' + id + ':' + canForAll
            })
            // The wait before a message goes, turned down so this does not
            // have to sit through four seconds of it.
            view.pendingDelay = 60
            view.resendRequested.connect(function(id) { raised = 'resend:' + id })
            view.arrivedAtNewest.connect(function() { arrivals += 1 })
            return 'ok'
        }
        function raisedSignal() { return raised }
        // Whether Send again is offered for a row in this delivery state.
        // Clicking it says nothing about that: findIn reaches an invisible
        // item just as well, so the gate needs asking about directly --
        // and the row has to be laid out again first, which is why setting
        // it up and reading it are separate calls a tick apart.
        function resetToState(state) {
            rows.clear()
            rows.append({
                message_id: 1, text: 'one', is_outgoing: true,
                is_info: false, show_padlock: true, state: state,
                timestamp: 1700000000, day_number: 19675,
                sender_name: 'Me', sender_color: '#00875a',
                quote_text: '', quote_author: '', file_path: '',
                file_name: '', view_type: 'Text',
                image_width: 0, image_height: 0
            })
            return 'ok'
        }
        function resendVisible() {
            var row = findIn(loader.item, 'messageRow')
            if (!row || !row.menu) { return 'no-menu' }
            var item = findIn(row.menu, 'resendItem')
            return item ? '' + item.visible : 'missing'
        }
        function clearRaised() { raised = ''; return 'ok' }
        // What the page the menu leads to reports back.
        function confirm(messageId, forEveryone) {
            loader.item.confirmDelete(messageId, forEveryone)
            return 'ok'
        }
        function emptyModel() { rows.clear(); return '' + rows.count }
        // Destroys the delegate the menu belongs to, as a reload or a
        // reorder does.
        function removeRow(index) { rows.remove(index); return 'ok' }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        // The menu is a property of the row, not a visual child of it.
        function pickMenu(name) {
            var row = findIn(loader.item, 'messageRow')
            if (!row) { return 'missing:messageRow' }
            if (!row.menu) { return 'no-menu' }
            var item = findIn(row.menu, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function get(property) { return '' + loader.item[property] }
        function set(property, value) {
            loader.item[property] = value
            return '' + loader.item[property]
        }
        // Scrolling up into the history, the way a reader would: the view's
        // own call, so it re-anchors rather than being shoved by contentY.
        function toTop() { loader.item.positionViewAtBeginning(); return 'ok' }
        // Silica raises these around a drag. A row is measured as it comes
        // into view, so scrolling up grows contentHeight by itself.
        function beginDrag() { loader.item.movementStarted(); return 'ok' }
        function arrivedCount() { return '' + arrivals }
        function toBottom() { loader.item.positionViewAtEnd(); return 'ok' }
        function jump() { loader.item.jumpToNewest(); return 'ok' }
        // Silica sends this when a drag or flick settles.
        function settle() { loader.item.movementEnded(); return 'ok' }
        // The view's own answer to 'is the end on screen'.
        function ended() { return '' + loader.item.atYEnd }
        // Stop a little short of the end, the way a real scroll does while
        // rows are still being measured.
        function stopJustShort(gap) {
            var view = loader.item
            view.positionViewAtEnd()
            view.contentY = view.contentY - gap
            return '' + view.atYEnd
        }
        function near() { return '' + loader.item.nearBottom }
        // What coming back from a picture does: hold the row the reader
        // left on, against the rows settling.
        function holdAt(index) { loader.item.holdAt(index); return 'ok' }
        // The view moved down a little by something other than the
        // reader: the view's own clamp, or a flick's last frame.
        function nudge(gap) {
            var view = loader.item
            view.contentY = view.contentY + gap
            return 'ok'
        }
        // The last row grows, as it does when its picture decodes.
        function growLast() {
            var text = 'a message that has grown'
            for (var i = 0; i < 8; i++) { text += text }
            rows.setProperty(rows.count - 1, 'text', text)
            return 'ok'
        }
        // The placeholder's own enabled state, read off the item rather
        // than recomputed -- a probe that repeated the binding would pass
        // whatever the component did.
        function setLoaded(value) { loader.item.loaded = value; return 'ok' }
        function placeholderOn() {
            var item = findIn(loader.item, 'emptyPlaceholder')
            return item ? '' + item.enabled : 'missing:emptyPlaceholder'
        }
    }
";

// Timed steps in order.
#[allow(clippy::too_many_lines)]
#[test]
fn a_conversation_opens_at_the_newest_message_and_stays_where_it_is_left() {
    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        // History longer than view, as on reopen.
        record!("filled", call!("append", 40));
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("ConversationList.qml"))
            )
        );
    });

    plan.after(1, move || unsafe {
        record!("opened-at-end", call!("ended"));
        record!(
            "opened-sticky",
            call!("get", QString::from("stickToBottom"))
        );
        // New message while reader at bottom.
        call!("append", 1);
    });

    plan.after(1, move || unsafe {
        record!("followed", call!("ended"));

        // Reader scrolls up, stops there.
        call!("toTop");
        call!("settle");
        record!(
            "scrolled-sticky",
            call!("get", QString::from("stickToBottom"))
        );
        call!("append", 1);
    });

    plan.after(1, move || unsafe {
        record!("stayed-put", call!("ended"));

        // Reader scrolls back to newest.
        call!("toBottom");
        call!("settle");
        record!(
            "returned-sticky",
            call!("get", QString::from("stickToBottom"))
        );
        call!("append", 1);
    });

    plan.after(1, move || unsafe {
        record!("resumed", call!("ended"));

        // Menu raises what page then acts on.
        record!(
            "reply-picked",
            call!("pickMenu", QString::from("replyItem"))
        );
        record!("reply", call!("raisedSignal"));
        call!("pickMenu", QString::from("copyItem"));
        record!("copy", call!("raisedSignal"));
        call!("pickMenu", QString::from("resendItem"));
        record!("resend", call!("raisedSignal"));
        // Delete asks kind; page answers.
        call!("pickMenu", QString::from("deleteItem"));
        record!("delete-ask", call!("raisedSignal"));
        // List waits after answer: delete itself read next step.
        call!("confirm", 1, false);

        // Scrolled away: arrival counted.
        call!("toTop");
        call!("settle");
        call!("append", 2);
    });

    plan.after(1, move || unsafe {
        record!("delete", call!("raisedSignal"));
        record!("missed", call!("get", QString::from("missedCount")));

        // Reader's own message not missed.
        call!("appendOwn");
        record!(
            "missed-after-own",
            call!("get", QString::from("missedCount"))
        );

        call!("jump");
    });

    plan.after(1, move || unsafe {
        record!("jumped", call!("ended"));
        record!("missed-cleared", call!("get", QString::from("missedCount")));
        record!("arrived", call!("arrivedCount"));

        // Following newest; reader drags up.
        call!("beginDrag");
        record!("held-follows", call!("get", QString::from("following")));
        call!("toTop");
        // Mid-drag row or arrival moves `contentHeight`, hauls reader down.
        call!("append", 1);
    });

    plan.after(1, move || unsafe {
        record!("held-stayed", call!("ended"));

        // Away from newest, flick still gliding when jump tapped. Button isn't part of list, so
        // nothing else stops the flick.
        call!("toTop");
        call!("settle");
        call!("beginDrag");
        call!("jump");
    });

    plan.after(1, move || unsafe {
        record!("jumped-mid-flick", call!("ended"));

        // Picked, then row destroyed before countdown ends (as reload, reorder, or earlier delete
        // do). Countdown belongs to list, not row, so row going takes nothing. Cleared first: stale
        // value would read as fresh and assertion hold either way.
        call!("clearRaised");
        call!("pickMenu", QString::from("deleteItem"));
        call!("confirm", 1, false);
        call!("removeRow", 0);
    });

    plan.after(1, move || unsafe {
        record!("delete-after-removal", call!("raisedSignal"));

        // Last: replaces model contents.
        call!("resetToState", 24);
    });

    plan.after(1, move || unsafe {
        record!("resend-when-failed", call!("resendVisible"));
        // Same row is account's own and encrypted: ask says core would take delete for everyone.
        call!("clearRaised");
        call!("pickMenu", QString::from("deleteItem"));
        record!("delete-ask-own", call!("raisedSignal"));
        call!("resetToState", 26);
    });

    plan.after(1, move || unsafe {
        record!("resend-when-delivered", call!("resendVisible"));
        // Refill: `resetToState` left a handful of rows; list too short to scroll is near bottom by
        // definition, nothing to scroll away from.
        call!("append", 40);
        call!("beginDrag");
        call!("toTop");
        call!("settle");
    });

    plan.after(1, move || unsafe {
        record!("away", call!("get", QString::from("stickToBottom")));
        call!("beginDrag");
        record!("short-atyend", call!("stopJustShort", 12.0));
        record!("short-near", call!("near"));
        call!("settle");
    });

    plan.after(1, move || unsafe {
        record!("short-sticks", call!("get", QString::from("stickToBottom")));
        // Empty, not-yet-loaded: as before messages arrive.
        call!("emptyModel");
        call!("setLoaded", false);
    });

    plan.after(1, move || unsafe {
        record!("placeholder-while-loading", call!("placeholderOn"));
        call!("setLoaded", true);
    });

    plan.after(1, move || unsafe {
        record!("placeholder-when-empty", call!("placeholderOn"));
        // Refilled for jumps below.
        call!("append", 40);
    });

    plan.after(1, move || unsafe {
        // Back from a picture, held on a row up in history; jump tapped before hold runs out. Row
        // measured after jump would put reader straight back.
        call!("holdAt", 20);
        call!("jump");
        call!("append", 1);
    });

    plan.after(1, move || unsafe {
        record!("jumped-from-hold", call!("ended"));
        record!(
            "sticky-from-hold",
            call!("get", QString::from("stickToBottom"))
        );

        // Up in history, jump tapped, view moves down a little on its own before jump lands.
        // Moving down is never reader leaving.
        call!("toTop");
        call!("settle");
        call!("jump");
        call!("nudge", 50.0);
    });

    plan.after(1, move || unsafe {
        record!("jumped-despite-nudge", call!("ended"));
        record!(
            "sticky-despite-nudge",
            call!("get", QString::from("stickToBottom"))
        );

        // At newest, which then grows (picture decoding, placeholder filled): reader still at
        // bottom of it.
        call!("growLast");
    });

    plan.after(1, move || unsafe {
        record!("grown-end", call!("ended"));
        record!("grown-sticky", call!("get", QString::from("stickToBottom")));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    assert_outcome(&steps);
    assert_near_bottom(&steps);
    assert_jumps_land(&steps);
}

/// Jump goes all the way, whatever else moves the view.
fn assert_jumps_land(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("jumped-from-hold"),
        "true",
        "jumping to the newest message while a row was held went to the \
         end and was put straight back on the held row by the next row \
         measured -- with the button already gone. {context}"
    );
    assert_eq!(
        value("sticky-from-hold"),
        "true",
        "the jump from a held row did not leave the view following. {context}"
    );
    assert_eq!(
        value("jumped-despite-nudge"),
        "true",
        "the view moving down a little between the tap and the jump was \
         taken for the reader scrolling away, and the jump was dropped. \
         {context}"
    );
    assert_eq!(
        value("sticky-despite-nudge"),
        "true",
        "a small move down while following switched following off. {context}"
    );
    assert_eq!(
        value("grown-end"),
        "true",
        "the newest message growing under a view at the end left the view \
         at the top of it: a tall picture shown from its upper half. {context}"
    );
    assert_eq!(
        value("grown-sticky"),
        "true",
        "the newest message growing switched following off. {context}"
    );
}

/// Opening lands on newest; arrival moves view only if reader there. Stop just short counts as
/// arrival, else jump-to-newest button can't be dismissed.
// One assertion per thing, in step order.
#[allow(clippy::too_many_lines)]
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    assert_eq!(value("filled"), "40", "the model did not fill. {context}");
    assert_eq!(
        value("opened-at-end"),
        "true",
        "reopening a chat put the reader back at the top of the history. {context}"
    );
    assert_eq!(
        value("opened-sticky"),
        "true",
        "a freshly opened chat does not follow new messages. {context}"
    );
    assert_eq!(
        value("followed"),
        "true",
        "a message arriving while the reader was at the bottom did not scroll in. {context}"
    );
    assert_eq!(
        value("scrolled-sticky"),
        "false",
        "scrolling up into the history left the view still following. {context}"
    );
    assert_eq!(
        value("stayed-put"),
        "false",
        "a message arriving pulled the reader out of the history. {context}"
    );
    assert_eq!(
        value("returned-sticky"),
        "true",
        "scrolling back to the newest message did not resume following. {context}"
    );
    assert_eq!(
        value("resumed"),
        "true",
        "the next message did not scroll in after following resumed. {context}"
    );

    assert_eq!(
        value("reply-picked"),
        "ok",
        "the row has no context menu. {context}"
    );
    // Reply carries what page needs.
    assert_eq!(
        value("reply"),
        "reply:1:message number 0, long enough to take a line of its own in the list:Ada",
        "Reply did not name the message it is replying to. {context}"
    );
    assert!(
        value("copy").starts_with("copy:message number 0"),
        "Copy did not carry the message's text: {}. {context}",
        value("copy")
    );
    assert_eq!(
        value("delete-ask"),
        "ask:1:false",
        "Delete did not ask which kind of delete it is, or did not name \
         its message. Somebody else's message is not one the core will \
         delete for everyone. {context}"
    );
    assert_eq!(
        value("delete-ask-own"),
        "ask:1:true",
        "the ask does not offer to delete this account's own encrypted \
         message for everyone, which is exactly what the core takes. \
         {context}"
    );
    assert_eq!(
        value("delete"),
        "delete:1:false",
        "Delete did not name its message, or lost which kind of delete \
         the reader picked on the way through the wait. {context}"
    );
    assert_eq!(
        value("resend"),
        "resend:1",
        "Send again did not name its message. {context}"
    );

    assert_eq!(
        value("delete-after-removal"),
        "delete:1:false",
        "a delete whose row was destroyed mid-countdown never arrived. \
         The wait belongs to the list, not to the row: a row is destroyed \
         by the very thing this is used for, which is deleting the \
         message above it. {context}"
    );

    // `DC_STATE_OUT_FAILED`: only state worth offering resend. Click proves wiring; whether reader
    // can reach it is not asked.
    assert_eq!(
        value("resend-when-failed"),
        "true",
        "Send again is hidden on a message that failed, which is the one \
         case it exists for. {context}"
    );
    assert_eq!(
        value("resend-when-delivered"),
        "false",
        "Send again is offered on a message that was delivered. {context}"
    );

    assert_eq!(
        value("missed"),
        "2",
        "messages arriving out of sight were not counted. {context}"
    );
    assert_eq!(
        value("missed-after-own"),
        "2",
        "a message the reader sent themselves was counted as one they \
         missed, which is what differencing the row count does. {context}"
    );
    assert_eq!(
        value("held-follows"),
        "false",
        "the view still follows while the reader has hold of it, which hauls \
         them back down the moment they scroll up. {context}"
    );
    // Jump must move view even if tapped mid-flick: it clears missed badge and tells page newest
    // was reached; doing that without scrolling marks read messages still out of sight.
    assert_eq!(
        value("jumped-mid-flick"),
        "true",
        "tapping jump-to-newest during a flick left the view up in the \
         history, having already cleared the badge and reported arrival. \
         {context}"
    );

    assert_eq!(
        value("held-stayed"),
        "false",
        "the reader was dragged back to the newest message mid-drag. {context}"
    );
    assert!(
        value("arrived").parse::<i32>().unwrap_or_default() >= 1,
        "reaching the newest message was not announced, so nothing marks it \
         read. {context}"
    );
    assert_eq!(
        value("jumped"),
        "true",
        "jumping to the newest message did not move the view. {context}"
    );
    assert_eq!(
        value("missed-cleared"),
        "0",
        "the count of missed messages survived jumping to them. {context}"
    );
}

/// Stopping just short: arrived; jump button goes.
fn assert_near_bottom(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("away"),
        "false",
        "scrolling up did not clear stickToBottom, so nothing below is under \
         test. {context}"
    );
    // Arrangement: genuinely short of end by view's reckoning, else not the reported case.
    assert_eq!(
        value("short-atyend"),
        "false",
        "stopping short still counted as atYEnd, so this no longer \
         reproduces the button that would not go away. {context}"
    );
    assert_eq!(
        value("short-near"),
        "true",
        "a reader a few pixels from the end is not counted as near it. {context}"
    );
    assert_eq!(
        value("placeholder-while-loading"),
        "false",
        "\"no messages yet\" is shown while the chat is still loading, which \
         is the flash of an empty chat seen when opening a busy one. {context}"
    );
    assert_eq!(
        value("placeholder-when-empty"),
        "true",
        "a chat that really is empty says nothing at all. {context}"
    );
    assert_eq!(
        value("short-sticks"),
        "true",
        "settling just short of the newest message left the jump button up, \
         with no way to dismiss it. {context}"
    );
}
