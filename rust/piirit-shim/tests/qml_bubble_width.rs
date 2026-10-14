//! How wide a message's bubble is, worked out without measuring more
//! than the bubble shows.
//!
//! A bubble is as wide as its widest line, up to most of the screen. The
//! body used to be measured whole -- every line of a pasted log laid out
//! once more, off screen, for each row built -- when the bubble shows a
//! dozen. What this pins is that measuring only those leaves the bubble
//! the width it had: a remark, a paragraph that wraps, a few lines of
//! different lengths, one of them wrapping, a dozen with the widest last,
//! the shim's rendering, and a body with an emoji picture in it. Each is
//! held against a copy of the body measured whole, as before.
//!
//! And that a picture's bubble is the full width from the start. The
//! row asked the attachment whether it wanted the full width, and the
//! attachment is built by a loader after the row: until it was, the
//! answer was no, so a picture was first sized for the width of its
//! caption and then again for the bubble's. The row answers for itself
//! now, from what the attachment answers from -- and the two must agree
//! for every kind there is.
//!
//! The delegate is loaded on its own, as the other QML tests load it.

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
use serde_json::{json, Value};

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 900

        Loader { id: delegate }
        // The body as the bubble used to measure it: every line,
        // unwrapped, however many there are.
        Text { id: whole; visible: false }

        // A fresh row each time, as a list builds one, with its
        // properties as JSON.
        function load(url, json) {
            var props = JSON.parse(json)
            props.width = 540
            delegate.source = ''
            delegate.setSource(url, props)
            return delegate.status === Loader.Ready ? 'ok' : 'load-failed'
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
        // The width the body is wrapped at, and the one measuring the
        // whole body gives, capped as the bubble caps it: 'body/whole'.
        function bodyWidths() {
            var row = delegate.item
            var label = findIn(row, 'messageLabel')
            if (!label) { return 'missing:messageLabel' }
            whole.font = label.font
            whole.textFormat = label.textFormat
            whole.text = label.text
            return row.bodyWidth.toFixed(2) + '/'
                   + Math.min(row.maxWidth, whole.implicitWidth).toFixed(2)
        }
        // Whether the row has its attachment fill the bubble, whether the
        // attachment asks to ('-' for none built), and whether the bubble
        // is at its full width.
        function fills() {
            var row = delegate.item
            var preview = findIn(row, 'attachment')
            return row.attachmentFillsBubble + '/'
                   + (preview ? preview.wantsFullWidth : '-')
                   + '/' + (row.bodyWidth === row.maxWidth)
        }
    }
";

/// Words enough to wrap.
fn words(count: usize, word: &str) -> String {
    vec![word; count].join(" ")
}

/// `count` lines, the one at `widest` much longer than the rest.
fn lines(count: usize, widest: usize) -> String {
    (0..count)
        .map(|line| {
            if line == widest {
                "the widest line of all of them".to_string()
            } else {
                format!("line {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
#[allow(clippy::too_many_lines)]
fn bubbles_measure_what_they_show_and_pictures_fill_them_from_the_start() {
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
    let mut steps: Vec<(String, String)> = Vec::new();
    let steps_ptr: *mut Vec<(String, String)> = std::ptr::addr_of_mut!(steps);

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
            (*steps_ptr).push(($label.to_string(), $value))
        };
    }

    let bodies: Vec<(&'static str, Value)> = vec![
        (
            "remark",
            json!({ "messageText": "A remark of a few words" }),
        ),
        ("paragraph", json!({ "messageText": words(60, "word") })),
        ("just-over", json!({ "messageText": words(16, "abcd") })),
        (
            "few-lines",
            json!({ "messageText": "hi\na much longer second line here\nok" }),
        ),
        (
            "few-lines-one-wraps",
            json!({ "messageText": format!("hi\n{}\nok", words(40, "ab")) }),
        ),
        ("dozen-widest-last", json!({ "messageText": lines(12, 11) })),
        (
            "thirteen-widest-last",
            json!({ "messageText": lines(13, 12) }),
        ),
        (
            "styled",
            json!({
                "markdownMode": 0,
                "messageText": "**bold** words and a few more",
                "styledText": "<b>bold</b> words and a few more"
            }),
        ),
        (
            "emoji",
            json!({ "messageText": "a remark with a picture 👍 in it" }),
        ),
    ];
    let bodies_ptr = std::ptr::addr_of!(bodies);

    // Every kind of attachment, and the two webxdc rows that are not
    // drawn as an app: apps turned off, and one the core could not read.
    let attachments: Vec<(&'static str, Value)> = vec![
        ("image", json!({ "viewType": "Image" })),
        ("sticker", json!({ "viewType": "Sticker" })),
        ("gif", json!({ "viewType": "Gif" })),
        ("video", json!({ "viewType": "Video" })),
        ("audio", json!({ "viewType": "Audio" })),
        ("voice", json!({ "viewType": "Voice" })),
        ("vcard", json!({ "viewType": "Vcard", "vcardName": "Bob" })),
        (
            "app",
            json!({ "viewType": "Webxdc", "appsEnabled": true, "webxdcName": "Chess" }),
        ),
        (
            "app-off",
            json!({ "viewType": "Webxdc", "appsEnabled": false, "webxdcName": "Chess" }),
        ),
        (
            "app-unread",
            json!({ "viewType": "Webxdc", "appsEnabled": true }),
        ),
        ("file", json!({ "viewType": "File" })),
        ("text-file", json!({ "viewType": "Text" })),
    ];
    let attachments_ptr = std::ptr::addr_of!(attachments);

    single_shot(Duration::from_secs(1), move || unsafe {
        let url = common::component_url("MessageDelegate.qml");
        for (label, props) in &*bodies_ptr {
            record!(
                format!("{label}-load"),
                call!(
                    "load",
                    QString::from(url.as_str()),
                    QString::from(props.to_string())
                )
            );
            record!(*label, call!("bodyWidths"));
        }
        for (label, props) in &*attachments_ptr {
            let mut with_file = props.clone();
            with_file["filePath"] = json!("/tmp/piirit-bubble-width/a");
            with_file["fileName"] = json!("a");
            with_file["messageText"] = json!("look");
            record!(
                format!("{label}-load"),
                call!(
                    "load",
                    QString::from(url.as_str()),
                    QString::from(with_file.to_string())
                )
            );
            record!(*label, call!("fills"));
            // The same kind with no file: nothing to fill the bubble.
            record!(
                format!("{label}-bare-load"),
                call!(
                    "load",
                    QString::from(url.as_str()),
                    QString::from(props.to_string())
                )
            );
            record!(format!("{label}-bare"), call!("fills"));
        }
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    for (label, _) in &bodies {
        assert_eq!(
            value(&format!("{label}-load")),
            "ok",
            "{label} did not load. {context}"
        );
        let widths = value(label);
        let (body, whole) = widths.split_once('/').unwrap_or((widths.as_str(), ""));
        assert_eq!(
            body, whole,
            "the {label} bubble's body is {body} wide, where measuring the \
             whole body made it {whole:?}. {context}"
        );
    }

    let fill = [
        "image", "sticker", "gif", "video", "audio", "voice", "vcard", "app",
    ];
    for (label, _) in &attachments {
        for load in [format!("{label}-load"), format!("{label}-bare-load")] {
            assert_eq!(value(&load), "ok", "{load} failed. {context}");
        }
        let wanted = fill.contains(label);
        assert_eq!(
            value(label),
            format!("{wanted}/{wanted}/{wanted}"),
            "the row and the attachment disagree on whether a {label} fills \
             the bubble (row/attachment/bubble at full width). {context}"
        );
        assert_eq!(
            value(&format!("{label}-bare")),
            "false/-/false",
            "a {label} message with no file filled the bubble. {context}"
        );
    }
}
