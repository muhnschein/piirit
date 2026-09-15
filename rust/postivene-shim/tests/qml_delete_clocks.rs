//! Each message deleted goes when its own wait is up, not when the last
//! one's is.
//!
//! One countdown kept for everything waiting would have to be restarted on
//! each new delete, so the first message's countdown runs out, the platform
//! puts that message back, and both go together when the second countdown
//! ends. A single delete never shows the difference.
//!
//! Every id carries its own deadline, and the timer is armed for
//! whichever is soonest. What this pins is the shape a shared countdown
//! cannot produce: asked a second apart, the first is gone while the
//! second is still waiting.

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

        /// Every delete the list asked for, in the order it asked.
        property string sent: ''

        function load(url, count, delay) {
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
                    loaded: true
                })
            }
            list.setSource(url, { model: rows })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.deleteRequested.connect(function(id) {
                sent += id + ';'
            })
            list.item.pendingDelay = delay
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
        /// Delete, from that message's own menu, and answer the page it
        /// leads to: the menu asks which kind of delete this is
        /// (pages/DeleteMessageDialog.qml) and the wait starts on the
        /// answer, so both are one reader's one decision.
        function deleteRow(messageId) {
            var row = rowFor(messageId)
            if (!row) { return 'missing:row:' + messageId }
            var item = findIn(row.menu, 'deleteItem')
            if (!item) { return 'missing:deleteItem' }
            item.clicked()
            list.item.confirmDelete(messageId, false)
            return 'ok'
        }
        /// Which messages are still waiting, oldest id first. A shared
        /// countdown and a per-message one differ here a second in: with
        /// one clock both are still waiting, with two the first is gone.
        function stillWaiting() {
            var out = ''
            for (var i = 1; i <= rows.count; i++) {
                if (list.item.pendingFor(i)) { out += i + ';' }
            }
            return out
        }
    }
";

// A script of timed steps and the assertions that read them; splitting
// it would hide the order the steps run in, which is the whole point.
#[test]
#[allow(clippy::too_many_lines)]
fn a_delete_goes_when_its_own_wait_is_up() {
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

    // A second and a half, so that a delete asked for a second after
    // another lands squarely between the two deadlines.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("ConversationList.qml")),
                4,
                1500
            )
        );
    });

    // The first, due at 3.5s.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("ask-first", call!("deleteRow", 1));
        record!("waiting-after-first", call!("stillWaiting"));
    });

    // The second, a second later, due at 4.5s. Asking for it must not
    // touch the first one's deadline.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("nothing-sent-yet", call!("sentSoFar"));
        record!("ask-second", call!("deleteRow", 2));
        record!("waiting-after-second", call!("stillWaiting"));
    });

    // Between the two deadlines: the first has gone, the second has not.
    // One shared countdown restarted on the second ask would have sent
    // neither by now, and both at 4.5s.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("between", call!("sentSoFar"));
        record!("waiting-between", call!("stillWaiting"));
    });

    // And past the second deadline, both.
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("after-both", call!("sentSoFar"));
        record!("waiting-after-both", call!("stillWaiting"));
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
    assert_eq!(
        (value("ask-first"), value("ask-second")),
        ("ok".to_string(), "ok".to_string()),
        "a delete did not reach the row's menu. {context}"
    );

    assert_eq!(
        value("waiting-after-first"),
        "1;",
        "asking for one message went to more than one, or to none. \
         {context}"
    );
    assert_eq!(
        value("nothing-sent-yet"),
        "",
        "the message went before its wait was up. {context}"
    );
    assert_eq!(
        value("waiting-after-second"),
        "1;2;",
        "asking for a second message did not leave both waiting. \
         {context}"
    );

    assert_eq!(
        value("between"),
        "1;",
        "a second and a half after the first delete was asked for and \
         half a second after the second, the first message has not gone \
         on its own -- or the second has gone early. Each carries its own \
         deadline; one countdown shared between them and restarted on \
         every ask is what made the first message's timer run out, the \
         message come back as though nothing had happened, and both go \
         together at the end. {context}"
    );
    assert_eq!(
        value("waiting-between"),
        "2;",
        "the first message is still counted as waiting after it went, or \
         the second stopped waiting early. {context}"
    );

    assert_eq!(
        value("after-both"),
        "1;2;",
        "past both deadlines, both messages should have gone, in the \
         order they were asked for. {context}"
    );
    assert_eq!(
        value("waiting-after-both"),
        "",
        "something is still waiting after every deadline has passed. \
         {context}"
    );
}
