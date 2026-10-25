//! Message field grows past one line (return sends, so no paragraphs otherwise).
//! Stops growing before eating conversation above. Row off bottom edge; buttons level
//! with field, beside last line.

// Qt harness: see qml_conversation_open.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

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

/// Draft past core's body cut: 40 lines, limit 38.
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

    // SAFETY: single thread; pre-Qt.
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!("open", call!("open", QString::from(page.clone())));
    });

    plan.after(1, move || unsafe {
        record!("one-line-height", call!("fieldHeight"));
        // Paragraph: three lines; old field held none.
        record!("typed", call!("type", QString::from("one\ntwo\nthree")));
    });

    plan.after(1, move || unsafe {
        record!("three-line-height", call!("fieldHeight"));
        record!("long", call!("type", QString::from(long.clone())));
    });

    plan.after(1, move || unsafe {
        record!("capped-height", call!("fieldHeight"));
        // Back to short draft: row below measured at ordinary height, not cap.
        record!("shortened", call!("type", QString::from("never mind")));
    });

    plan.after(1, move || unsafe {
        record!("field-lift", call!("liftOf", QString::from("messageField")));
        record!("send-lift", call!("liftOf", QString::from("sendButton")));
        record!(
            "attach-lift",
            call!("liftOf", QString::from("attachButton"))
        );
        // Button lifted off bottom of shorter row drawn over bar above.
        record!("send-top", call!("rowTopOf", QString::from("sendButton")));
        (*engine_ptr).quit();
    });

    plan.run();
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
    // A third of page: past that, conversation written in is gone.
    assert!(
        number("capped-height") <= 320.0,
        "a forty-line draft grew the field to {} on a 960-tall page, \
         which leaves nothing of the chat above it. {context}",
        number("capped-height")
    );

    // Row position measured as rule, not pixels: stub heights differ from Silica.
    // Order: screen edge, field, buttons.
    let field_lift = number("field-lift");
    let send_lift = number("send-lift");
    let attach_lift = number("attach-lift");
    assert!(
        field_lift > 0.0,
        "the field sits on the bottom edge of the screen ({field_lift} \
         above it). {context}"
    );
    // Level with field's bottom, a little above underline (Silica keeps room under line
    // for descenders). Lifted higher, buttons float over row.
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
