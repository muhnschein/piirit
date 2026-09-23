//! Part of a message, copied from the page that shows it whole.
//!
//! The page draws the message in a Label, which is what carries the
//! Markdown and the links, and a Label cannot be selected in. "Select
//! text" in the pull-down swaps it for a read-only field holding the
//! message as written, all of it selected so the handles are up, and each
//! selection is copied as it is made. Copy still copies the whole of it.
//!
//! The stub field has no handles, so a selection is made the way the
//! page makes its first one: by asking the field to select.

// Qt harness: see chat_actions.rs.
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

/// What the fake core holds for the message it seeds as one the sending
/// core cut: the page asks for the rest, and this is the whole of it.
const WHOLE: &str = "Groceries\nmilk\nbread\nand a & sign";

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: page; width: 540; height: 900 }
        // Markdown on, as it is by default: the label draws the shim's
        // rendering, which is what a field cannot.
        function openPage(url, id) {
            page.setSource(url, {
                accountId: 1, messageId: id, senderName: 'Ada', markdownMode: 0
            })
            return page.status === Loader.Ready ? 'ok' : 'load-failed'
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
        function shown(name, property) {
            var item = findIn(page.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function click(name) {
            var item = findIn(page.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function select(name, start, end) {
            var item = findIn(page.item, name)
            if (!item) { return 'missing:' + name }
            item.select(start, end)
            return 'ok'
        }
        function clipboard() { return Clipboard.text }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_part_of_the_whole_message_can_be_selected_and_copied() {
    let temp = std::env::temp_dir().join(format!("piirit-select-message-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

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
    macro_rules! shown {
        ($name:expr, $property:expr) => {
            call!("shown", QString::from($name), QString::from($property))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    let page = common::page_url("MessagePage.qml");
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("page", call!("openPage", QString::from(page.clone()), 11));
    });

    // The whole text is in by now. The label shows it, and the field
    // waits until it is asked for.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("label-before", shown!("bodyLabel", "visible"));
        record!("field-before", shown!("selectableBody", "visible"));
        record!("offered", shown!("selectItem", "visible"));
        record!("offered-enabled", shown!("selectItem", "enabled"));
        record!("clipboard-before", call!("clipboard"));

        record!("choose", call!("click", QString::from("selectItem")));
        record!("label-after", shown!("bodyLabel", "visible"));
        record!("field-after", shown!("selectableBody", "visible"));
        record!("field-text", shown!("selectableBody", "text"));
        record!("field-read-only", shown!("selectableBody", "readOnly"));
        record!("field-focusable", shown!("selectableBody", "focusOnClick"));
        record!("selected-first", shown!("selectableBody", "selectedText"));
        record!("cursor-first", shown!("selectableBody", "cursorPosition"));
        record!("clipboard-all", call!("clipboard"));
        record!("notice", shown!("noticeLabel", "text"));
        record!("offered-after", shown!("selectItem", "visible"));

        // One word out of the middle, as the handles would leave it.
        record!(
            "select-part",
            call!("select", QString::from("selectableBody"), 10, 14)
        );
        record!("clipboard-part", call!("clipboard"));
        // Letting go of the selection does not empty the clipboard.
        record!(
            "select-none",
            call!("select", QString::from("selectableBody"), 0, 0)
        );
        record!("clipboard-kept", call!("clipboard"));

        // Copy is still the whole message, selection or not.
        record!("copy", call!("click", QString::from("copyItem")));
        record!("clipboard-copy", call!("clipboard"));
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

    for (label, expected, complaint) in [
        ("page", "ok", "the full-message page did not load"),
        (
            "label-before",
            "true",
            "the page does not open on the formatted message",
        ),
        (
            "field-before",
            "false",
            "the selectable field is up before anyone asked for it, in \
             place of the formatting and the links",
        ),
        ("offered", "true", "nothing offers to select the text"),
        (
            "offered-enabled",
            "true",
            "selecting is offered but cannot be chosen",
        ),
        ("choose", "ok", "there is no Select text entry to tap"),
        (
            "label-after",
            "false",
            "the label is still drawn over the field it gave way to",
        ),
        (
            "field-after",
            "true",
            "choosing Select text put up nothing to select in",
        ),
        (
            "field-text",
            WHOLE,
            "the field does not hold the message as written, which is what \
             Copy copies",
        ),
        (
            "field-read-only",
            "true",
            "the field can be typed into, and the message is not the \
             reader's to edit",
        ),
        (
            "field-focusable",
            "true",
            "the read-only field takes no focus, and on the phone a field \
             without it cannot be selected in",
        ),
        (
            "selected-first",
            WHOLE,
            "the field comes up with nothing selected, so there are no \
             handles to move",
        ),
        (
            "cursor-first",
            "0",
            "the first selection leaves the cursor at the end of the \
             message, and the field scrolls the page to keep its cursor in \
             view: a long message would jump to its last line",
        ),
        ("clipboard-all", WHOLE, "the first selection was not copied"),
        ("notice", "Copied", "copying a selection says nothing"),
        (
            "offered-after",
            "false",
            "Select text is still offered once the field is up",
        ),
        ("select-part", "ok", "the field cannot be selected in"),
        (
            "clipboard-part",
            "milk",
            "a part of the message was selected and not copied",
        ),
        (
            "clipboard-kept",
            "milk",
            "letting go of a selection emptied the clipboard",
        ),
        ("copy", "ok", "there is no Copy entry to tap"),
        (
            "clipboard-copy",
            WHOLE,
            "Copy no longer copies the whole message",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
    assert_eq!(
        value("clipboard-before"),
        "",
        "something was copied before anything was selected. {context}"
    );
    let _ = std::fs::remove_dir_all(&temp);
}
