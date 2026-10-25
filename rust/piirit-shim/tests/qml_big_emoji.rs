//! Message of few emoji, nothing else, drawn twice text size (Emoji.js `onlyEmoji`), as other
//! messengers do. Pinned: what counts (whitespace between ok, at most `bigLimit`, only emoji with
//! pictures, not text-style signs), and that body label and its pictures grow, caption doesn't.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use qmetaobject::*;
use serde_json::{json, Value};

mod common;

/// Probe imports script by absolute URL: data has no dir to resolve.
fn probe_qml() -> String {
    let qml = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    format!(
        r"
    import QtQuick 2.0
    import 'file://{qml}/js/Emoji.js' as Emoji
    Item {{
        width: 540
        height: 900

        function onlyEmoji(text) {{ return '' + Emoji.onlyEmoji(text) }}

        Loader {{ id: delegate }}
        function findIn(node, name) {{
            if (!node) {{ return null }}
            if (node.objectName === name) {{ return node }}
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {{
                var hit = findIn(kids[i], name)
                if (hit) {{ return hit }}
            }}
            return null
        }}
        // Fresh row, props as JSON: body font px, then picture edge in body ('-' if none).
        function body(url, json) {{
            var props = JSON.parse(json)
            props.width = 540
            delegate.source = ''
            delegate.setSource(url, props)
            if (delegate.status !== Loader.Ready) {{ return 'load-failed' }}
            var label = findIn(delegate.item, 'messageLabel')
            if (!label) {{ return 'missing:messageLabel' }}
            var edge = /width=.(\d+)/.exec(label.text)
            return label.font.pixelSize + '/' + (edge ? edge[1] : '-')
        }}
    }}
",
        qml = qml.display()
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn few_emoji_alone_are_drawn_twice_as_big() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(probe_qml().as_str()));

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

    let texts: Vec<(&'static str, &'static str, &'static str)> = vec![
        ("one", "👍", "true"),
        ("five", "😀😀😀😀😀", "true"),
        ("six", "😀😀😀😀😀😀", "false"),
        ("spaced", " 🎉\n🎉  🎉 ", "true"),
        ("joined", "👨\u{200d}👩\u{200d}👧 🇫🇮", "true"),
        ("toned", "👍🏽", "true"),
        ("keycap", "1\u{fe0f}\u{20e3}", "true"),
        ("with-word", "👍 ok", "false"),
        ("word-first", "ok 👍", "false"),
        ("punctuation", "👍!", "false"),
        ("digit", "1", "false"),
        ("copyright-text", "©", "false"),
        ("star-text", "\u{2b50}\u{fe0e}", "false"),
        ("blank", "  \n ", "false"),
        ("empty", "", "false"),
        ("plain", "hello", "false"),
    ];
    let texts_ptr = std::ptr::addr_of!(texts);

    // Big: 2 x stub fontSizeMedium 18; picture edge 1.15 x font px (Emoji.js `inText`). Else
    // Label's own size, untouched: "normal".
    let rows: Vec<(&'static str, Value, &'static str)> = vec![
        ("big", json!({ "messageText": "👍👍" }), "36/41"),
        // Shim's rendering joins lines with `<br>`: judged on text as sent.
        (
            "big-styled",
            json!({ "markdownMode": 0, "messageText": "🔥\n🔥", "styledText": "🔥<br>🔥" }),
            "36/41",
        ),
        ("mixed", json!({ "messageText": "hi 👍" }), "normal"),
        ("text", json!({ "messageText": "hi there" }), "normal"),
        (
            "caption",
            json!({ "messageText": "👍", "viewType": "File", "filePath": "/nonexistent/a.txt" }),
            "normal",
        ),
    ];
    let rows_ptr = std::ptr::addr_of!(rows);

    single_shot(Duration::from_secs(1), move || unsafe {
        for (label, text, _) in &*texts_ptr {
            (*steps_ptr).push((
                (*label).to_string(),
                call!("onlyEmoji", QString::from(*text)),
            ));
        }
        for (label, props, _) in &*rows_ptr {
            (*steps_ptr).push((
                (*label).to_string(),
                call!(
                    "body",
                    QString::from(common::component_url("MessageDelegate.qml")),
                    QString::from(props.to_string())
                ),
            ));
        }
        (*engine_ptr).quit();
    });

    engine.exec();

    let context = format!("steps: {steps:?}");
    for (label, text, expected) in &texts {
        assert_eq!(
            common::value_of(&steps, label),
            *expected,
            "{label}: is {text:?} only a few emoji? {context}"
        );
    }
    // Label's own size, from row without emoji.
    let normal = common::value_of(&steps, "text")
        .split_once('/')
        .and_then(|(px, _)| px.parse::<f64>().ok())
        .unwrap_or(0.0);
    assert!(
        normal > 0.0 && normal < 36.0,
        "plain text row's font is {normal} px. {context}"
    );
    for (label, props, expected) in &rows {
        let wanted = if *expected == "normal" {
            let has_emoji = props["messageText"] != "hi there";
            #[allow(clippy::cast_possible_truncation)]
            let edge = (normal * 1.15).round() as i64;
            format!(
                "{normal}/{}",
                if has_emoji {
                    edge.to_string()
                } else {
                    "-".into()
                }
            )
        } else {
            (*expected).to_string()
        };
        assert_eq!(
            common::value_of(&steps, label),
            wanted,
            "{label}: body font px / picture edge. {context}"
        );
    }
}
