//! When a message's menu offers Edit.
//!
//! Only where the core would take the edit, which is the rule
//! deltachat-android applies before offering it: a message of one's own,
//! not a notice, not a call, with text to change, and not one the
//! sending core cut -- and only in a chat that takes messages and is
//! encrypted, which the page knows and the list is told. The offer
//! carries the message's text with it, since that is what the field is
//! filled from.

// Qt harness: see qml_attachment_menu.rs.
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
        height: 1200

        property string raised: ''

        ListModel { id: rows }
        Loader { id: list; width: 540; height: 1200 }
        function row(id, outgoing, info, text, html, kind) {
            return {
                message_id: id, text: text, styled_text: '',
                is_edited: false, is_outgoing: outgoing, is_info: info,
                show_padlock: true, state: 26, timestamp: 1700000000 + id,
                day_number: 19675, sender_name: 'Ada',
                sender_color: '#00875a', is_forwarded: false,
                quote_text: '', quote_author: '', file_path: '',
                file_name: '', file_mime: '', file_bytes: 0,
                view_type: kind, image_width: 0, image_height: 0,
                is_new: false, has_html: html,
                download_state: 'Done', vcard_name: '', vcard_addr: '',
                vcard_color: '', webxdc_name: '', webxdc_document: '',
                webxdc_summary: '', webxdc_icon: '', reactions: '',
                my_reaction: '', loaded: true
            }
        }
        function load(url) {
            // Ours, with words: the one case that is offered.
            rows.append(row(1, true, false, 'my words', false, 'Text'))
            // Somebody else's.
            rows.append(row(2, false, false, 'their words', false, 'Text'))
            // A notice, which nobody wrote.
            rows.append(row(3, true, true, 'You left', false, 'Text'))
            // Ours, with no words: a picture sent bare.
            rows.append(row(4, true, false, '', false, 'Image'))
            // Ours, cut by the sending core.
            rows.append(row(5, true, false, 'long [...]', true, 'Text'))
            // A call.
            rows.append(row(6, true, false, 'Call', false, 'Call'))
            list.setSource(url, { model: rows, canEdit: true })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.editRequested.connect(function(messageId, body) {
                raised += 'edit:' + messageId + ':' + body + ';'
            })
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
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
        function rowsInOrder() {
            var found = findAll(list.item, 'messageRow', [])
            var seen = []
            for (var i = 0; i < found.length; i++) {
                if (seen.indexOf(found[i]) === -1) { seen.push(found[i]) }
            }
            seen.sort(function(a, b) { return a.y - b.y })
            return seen
        }
        /// Whether Edit is on offer on each row, oldest first, as a
        /// string of t and f.
        function offers() {
            var rows = rowsInOrder()
            var out = ''
            for (var i = 0; i < rows.length; i++) {
                var item = findIn(rows[i].menu, 'editItem')
                out += item ? (item.visible ? 't' : 'f') : '?'
            }
            return out
        }
        function labelOf() {
            var item = findIn(rowsInOrder()[0].menu, 'editItem')
            return item ? item.text : 'missing'
        }
        function pick() {
            var item = findIn(rowsInOrder()[0].menu, 'editItem')
            if (!item) { return 'missing' }
            item.clicked()
            return 'ok'
        }
        function setCanEdit(on) { list.item.canEdit = on; return 'ok' }
        function raisedSignal() { return raised }
    }
";

#[test]
fn edit_is_offered_on_what_the_core_would_take_an_edit_of() {
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
                QString::from(common::component_url("ConversationList.qml"))
            )
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("offers", call!("offers"));
        record!("label", call!("labelOf"));
        record!("picked", call!("pick"));
        record!("raised", call!("raisedSignal"));
        record!("chat-closed", call!("setCanEdit", false));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("offers-closed", call!("offers"));
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
        value("offers"),
        "tfffff",
        "Edit is offered on the wrong messages: in order, ours with words, \
         somebody else's, a notice, ours with no words, ours cut by the \
         sending core, a call. {context}"
    );
    assert_eq!(
        value("label"),
        "Edit",
        "the offer does not say what it does. {context}"
    );
    assert_eq!(
        value("raised"),
        "edit:1:my words;",
        "picking Edit did not name the message and carry its text. {context}"
    );
    assert_eq!(
        value("offers-closed"),
        "ffffff",
        "Edit is offered in a chat that takes no messages, where the core \
         would refuse it. {context}"
    );
}
