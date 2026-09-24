//! Writing a message longer than one line.
//!
//! The field a message is written in takes more than a line: with the
//! return key sending, a message written here would be one line however
//! long it ran, and a paragraph could not be typed at all. It grows with
//! the draft and stops growing before it has eaten the conversation
//! above it.
//!
//! And where the row sits: off the bottom edge rather than on it, with
//! the two round buttons level with the field rather than floating
//! above it, and beside the *last* line of a draft that has grown
//! rather than its first.

// Qt harness: see qml_conversation_open.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    Item {
        width: 540
        height: 960

        Loader { id: loader; width: 540; height: 960 }
        function open(url) {
            loader.setSource(url, {
                accountId: 1, chatId: 1, status: PageStatus.Active
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function type(text) {
            var field = findIn(loader.item, 'messageField')
            if (!field) { return 'missing:messageField' }
            field.text = text
            return 'ok'
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function fieldHeight() {
            var field = findIn(loader.item, 'messageField')
            return field ? '' + Math.round(field.height) : 'missing'
        }
        /// Where the bottom edge of something in the row is, measured
        /// from the bottom of the page: bigger means higher up.
        function liftOf(name) {
            var item = findIn(loader.item, name)
            var row = findIn(loader.item, 'inputRow')
            if (!item || !row) { return 'missing:' + name }
            var bottom = row.y + item.y + item.height
            return '' + Math.round(loader.item.height - bottom)
        }
        function rowTopOf(name) {
            var item = findIn(loader.item, name)
            return item ? '' + Math.round(item.y) : 'missing:' + name
        }
    }
";

/// A draft past the point where the core cuts a body: 40 lines, where
/// its limit is 38.
fn a_long_draft() -> String {
    let mut out = String::new();
    for number in 1..=40 {
        use std::fmt::Write as _;
        let _ = writeln!(out, "- [ ] the {number}th thing to do today");
    }
    out
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_field_takes_more_than_a_line_and_sits_above_the_edge() {
    let temp = std::env::temp_dir().join(format!("piirit-long-draft-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);
    let page = common::page_url_in(&tree, "ConversationPage.qml");

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

    let long = a_long_draft();

    single_shot(Duration::from_secs(1), move || unsafe {
        record!("open", call!("open", QString::from(page.clone())));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("one-line-height", call!("fieldHeight"));
        // A paragraph: three lines, which the old field could not hold
        // at all.
        record!("typed", call!("type", QString::from("one\ntwo\nthree")));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("three-line-height", call!("fieldHeight"));
        record!("long", call!("type", QString::from(long.clone())));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("capped-height", call!("fieldHeight"));
        // Back to a short draft, so the row below is measured at its
        // ordinary height rather than at the cap.
        record!("shortened", call!("type", QString::from("never mind")));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("field-lift", call!("liftOf", QString::from("messageField")));
        record!("send-lift", call!("liftOf", QString::from("sendButton")));
        record!(
            "attach-lift",
            call!("liftOf", QString::from("attachButton"))
        );
        // A button lifted off the bottom of a row shorter than it would
        // be drawn above the row, over the bar above it.
        record!("send-top", call!("rowTopOf", QString::from("sendButton")));
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
    let number = |label: &str| value(label).parse::<f64>().unwrap_or(-1.0);
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("open"),
        "ok",
        "the conversation page did not load. {context}"
    );

    let one_line = number("one-line-height");
    let three_lines = number("three-line-height");
    assert!(
        three_lines > one_line,
        "the field stayed {one_line} tall for a three-line message, so \
         the writer can see one line of what they are writing -- and the \
         line breaks in it are not line breaks at all. {context}"
    );
    // A third of the page: past that the conversation being written in
    // would be gone.
    assert!(
        number("capped-height") <= 320.0,
        "a forty-line draft grew the field to {} on a 960-tall page, \
         which leaves nothing of the chat above it. {context}",
        number("capped-height")
    );

    // Where the row sits. Measured as a rule rather than as pixels: what
    // a stub is tall is nothing like what Silica is, and what matters is
    // the order -- the screen's edge, then the field, then the buttons.
    let field_lift = number("field-lift");
    let send_lift = number("send-lift");
    let attach_lift = number("attach-lift");
    assert!(
        field_lift > 0.0,
        "the field sits on the bottom edge of the screen ({field_lift} \
         above it). {context}"
    );
    // Level with the field's bottom edge, which is a little above its
    // underline: a Silica field keeps room under its line for what hangs
    // below a letter. Lifted further, as this was first written, the
    // buttons read as floating over the row rather than belonging to it.
    assert!(
        (send_lift - field_lift).abs() < 1.0 && (attach_lift - field_lift).abs() < 1.0,
        "the send and attach buttons ({send_lift}, {attach_lift} above \
         the edge) do not sit with the field ({field_lift}). {context}"
    );
    assert!(
        number("send-top") >= 0.0,
        "the send button is drawn above the top of the row it is in, \
         which puts it over whatever bar sits above the row. {context}"
    );
}
