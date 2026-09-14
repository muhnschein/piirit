//! Deleting several messages, one after another.
//!
//! Each one waits a moment before it goes, so the reader can say they did
//! not mean it. That wait belongs beside the list, not to the row, where
//! `ListItem.remorseAction` would put it: deleting out of a list destroys
//! rows, and a wait beside the list cannot be lost that way. (Silica's
//! `RemorseItem` runs its callback rather than dropping it when the
//! countdown is cut short, so "the row went and took the wait with it" is
//! not the mechanism -- but a wait living somewhere that volatile has to
//! be proved every release.)
//!
//! So the wait belongs to the list, which outlives every row in it, and
//! what this pins is that nothing happening to the rows can lose a
//! delete: a row destroyed mid-countdown, a delete asked for while
//! others are already waiting, and the reader leaving the chat before
//! the wait is up.
//!
//! What draws the wait is Silica's own `RemorseItem` -- the bar, the
//! seconds, "Tap to cancel" -- raised by the row and handed a callback
//! that does nothing, because the deletion is the list's. So the way
//! back is a tap on the platform's countdown, and it puts back only the
//! one it covers.

// Qt harness: see qml_reactions.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 900

        ListModel { id: rows }
        Loader { id: list; width: 540; height: 900 }

        /// Every delete the list asked for, in order.
        property string sent: ''

        function load(url, count) {
            for (var i = 0; i < count; i++) {
                rows.append({
                    message_id: i + 1,
                    text: 'message number ' + (i + 1),
                    is_outgoing: false, is_info: false, show_padlock: true,
                    state: 16, timestamp: 1700000000 + i,
                    day_number: 19675, sender_name: 'Ada',
                    sender_color: '#00875a', quote_text: '',
                    quote_author: '', file_path: '', file_name: '',
                    view_type: 'Text', image_width: 0, image_height: 0,
                    // Without this the delegate's own `visible` is
                    // handed an undefined and keeps whatever it had, so
                    // a message on its way out is still drawn.
                    loaded: true
                })
            }
            list.setSource(url, { model: rows })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.deleteRequested.connect(function(id) {
                sent += id + ';'
            })
            // Long enough that a step of this test lands inside one
            // wait, short enough that it does not sit through four
            // seconds of the real one.
            list.item.pendingDelay = 2500
            return 'ok'
        }
        function sentSoFar() { return sent }

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
        function findAll(node, name, out) {
            if (!node) { return out }
            if (node.objectName === name) { out.push(node) }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                findAll(kids[i], name, out)
            }
            return out
        }
        /// The row showing this message. A row does not say which message
        /// it holds, but the menu's reaction strip does, and the menu
        /// belongs to the row.
        function rowFor(messageId) {
            var found = findAll(list.item, 'messageRow', [])
            for (var i = 0; i < found.length; i++) {
                var menu = found[i].menu
                var picker = menu ? findIn(menu, 'reactionPicker') : null
                if (picker && picker.messageId === messageId) {
                    return found[i]
                }
            }
            return null
        }
        /// Delete, from that message's own menu.
        function deleteRow(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var item = findIn(row.menu, 'deleteItem')
            if (!item) { return 'missing:deleteItem' }
            item.clicked()
            return 'ok'
        }
        /// Whether the platform's countdown is up over that message.
        ///
        /// Silica's own RemorseItem, not a stand-in: it fades what it
        /// covers with its own `opacity: 0.0`, so the message's opacity
        /// is what says the countdown took hold, and `enabled` is the
        /// row's own doing.
        function goingOut(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var remorse = findIn(row, 'messageRemorse')
            var body = findIn(row, 'messageDelegate')
            if (!body) { return 'missing:body' }
            // A row nobody has asked to delete has no countdown built at
            // all, which reads the same as one standing idle.
            return (remorse ? remorse.active : false) + '/' + body.opacity
                   + '/' + body.enabled
        }
        /// The row's own box: where it starts and how tall it is. A row
        /// waiting to go has to keep the height it had -- what replaces
        /// the message is centred in that height, and a row that
        /// collapsed to nothing puts its label across its neighbours'.
        function rowBox(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            return Math.round(row.y) + ':' + Math.round(row.height)
        }
        /// Whether the countdown covers the message and nothing else:
        /// Silica fills the item it was handed, so it must be no taller
        /// than the row it sits in.
        function remorseFits(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var remorse = findIn(row, 'messageRemorse')
            if (!remorse) { return 'missing:messageRemorse' }
            return (remorse.height <= row.height) + ':'
                   + Math.round(remorse.height) + '/' + Math.round(row.height)
        }
        /// What the countdown says it is doing.
        function goingText(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var remorse = findIn(row, 'messageRemorse')
            return remorse ? remorse.text : 'missing:messageRemorse'
        }
        /// A tap on the countdown, which is what calls a delete off.
        function tapRemorse(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var remorse = findIn(row, 'messageRemorse')
            if (!remorse) { return 'missing:messageRemorse' }
            remorse.tap()
            return 'ok'
        }
        // Destroys a delegate mid-countdown, as the core's own event does
        // once the delete before it has landed.
        function removeRow(index) { rows.remove(index); return 'ok' }
        function rowCount() { return '' + rows.count }
        // What the page does on its way out of the chat.
        function leaveChat() { list.item.flushDeletes(); return 'ok' }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn every_delete_in_a_run_arrives() {
    // SAFETY: single-threaded test binary; set before Qt starts.
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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("ConversationList.qml")),
                5
            )
        );
    });

    // A run: three deletes inside one wait, with the list changing under
    // them in the middle -- which is what the first delete of a run does
    // to the rows the rest are waiting on.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("box-1-before", call!("rowBox", 1));
        record!("box-2-before", call!("rowBox", 2));
        record!("delete-1", call!("deleteRow", 1));
        record!("delete-2", call!("deleteRow", 2));
        record!("going-1", call!("goingOut", 1));
        record!("going-text", call!("goingText", 1));
        record!("untouched-4", call!("goingOut", 4));
        record!("box-1-after", call!("rowBox", 1));
        record!("box-2-after", call!("rowBox", 2));
        record!("remorse-1-fits", call!("remorseFits", 1));
        record!("remorse-2-fits", call!("remorseFits", 2));
        record!("sent-so-far", call!("sentSoFar"));
        call!("removeRow", 0);
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("after-removal", call!("sentSoFar"));
        record!("delete-3", call!("deleteRow", 3));
        // And one the reader changes their mind about, in the same run.
        record!("delete-4", call!("deleteRow", 4));
        record!("spare-4", call!("tapRemorse", 4));
        record!("spared-4", call!("goingOut", 4));
        record!("still-going-3", call!("goingOut", 3));
    });

    // Past the wait: everything asked for has gone, and only that.
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("sent", call!("sentSoFar"));
        record!("back-4", call!("goingOut", 4));
        // The reader asks for one more and leaves the chat before the
        // wait is up.
        record!("delete-5", call!("deleteRow", 5));
        record!("leaving", call!("sentSoFar"));
        call!("leaveChat");
        record!("left", call!("sentSoFar"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    for label in ["delete-1", "delete-2", "delete-3", "delete-4", "delete-5"] {
        assert_eq!(
            value(label),
            "ok",
            "{label} did not reach the row's menu. {context}"
        );
    }

    assert_eq!(
        value("going-1"),
        "true/0/false",
        "the platform's countdown is not up over a message waiting to \
         go, or the message is still drawn under it, or its own controls \
         can still be tapped through it. {context}"
    );
    assert_eq!(
        value("going-text"),
        "Deleting",
        "a message on its way out does not say what is happening to it. \
         {context}"
    );
    assert_eq!(
        value("untouched-4"),
        "false/1/true",
        "asking for one message to go put a countdown over another. \
         {context}"
    );
    // The height a row had, kept: what replaces the message is centred in
    // it, and a row that collapses draws its label over its neighbours'.
    // Two of them waiting at once is where that shows.
    // Rounded to whole pixels on the way out of QML, so the boxes
    // compare as text: `y:height`, and both halves matter -- a row that
    // moved is as wrong as one that shrank.
    let height = |label: &str| -> i64 {
        value(label)
            .rsplit(':')
            .next()
            .and_then(|number| number.parse().ok())
            .unwrap_or(-1)
    };
    assert!(
        height("box-1-before") > 0 && height("box-2-before") > 0,
        "the rows were not measured before anything was deleted: {:?}, \
         {:?}. {context}",
        value("box-1-before"),
        value("box-2-before")
    );
    for message in [1, 2] {
        let before = value(&format!("box-{message}-before"));
        let after = value(&format!("box-{message}-after"));
        assert_eq!(
            before, after,
            "row {message}'s box changed when it was asked to go: \
             {before} became {after}. It has to keep the height it had, \
             or what replaces the message is centred in nothing and \
             drawn across the rows above and below. {context}"
        );
    }
    for label in ["remorse-1-fits", "remorse-2-fits"] {
        assert!(
            value(label).starts_with("true:"),
            "the countdown is taller than the row holding it ({}), so \
             it is drawn over its neighbours. {context}",
            value(label)
        );
    }

    assert_eq!(
        value("sent-so-far"),
        "",
        "a message went the moment it was asked for, so there was never \
         a moment to change your mind. {context}"
    );
    assert_eq!(
        value("after-removal"),
        "",
        "destroying a row sent the deletes early: the wait is the \
         reader's, and a row going away is not them changing their mind. \
         {context}"
    );

    assert_eq!(
        value("spare-4"),
        "ok",
        "there was no countdown over the message to tap. {context}"
    );
    assert_eq!(
        value("spared-4"),
        "false/1/true",
        "tapping the countdown over a message did not put the message \
         back. {context}"
    );
    assert_eq!(
        value("still-going-3"),
        "true/0/false",
        "putting one message back put another one back with it. {context}"
    );

    // The run: three asked for, one taken back, and a row destroyed in
    // the middle of all of it.
    let sent_text = value("sent");
    let sent: Vec<&str> = sent_text
        .split(';')
        .filter(|part| !part.is_empty())
        .collect();
    let mut ordered = sent.clone();
    ordered.sort_unstable();
    assert_eq!(
        ordered,
        ["1", "2", "3"],
        "a run of deletes did not all arrive, or one nobody asked for \
         did: {sent:?}. This is the whole bug -- the first delete of a \
         run destroys the rows the rest are waiting on. {context}"
    );
    assert_eq!(
        value("back-4"),
        "false/1/true",
        "the message that was put back went anyway. {context}"
    );

    assert_eq!(
        value("leaving"),
        value("sent"),
        "the last delete went before the wait was up. {context}"
    );
    assert_eq!(
        value("left"),
        format!("{}5;", value("sent")),
        "leaving the chat with a delete still waiting lost it. A reader \
         who asked for a message to go and then left asked for it to go. \
         {context}"
    );
}
