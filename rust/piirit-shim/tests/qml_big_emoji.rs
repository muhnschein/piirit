//! Message of few emoji, nothing else, drawn big (Emoji.js `onlyEmoji`), as other messengers
//! do. Pinned: what counts (whitespace between ok, at most `bigLimit`, only emoji with pictures,
//! not text-style signs); big ones laid out by hand (`bigLayout`): pictures near touching, sender's
//! space shows, line break kept, wrap at width, bubble as wide as pictures; caption and mixed text
//! stay inline at text size.

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
        function layout(text, edge, max) {{
            return JSON.stringify(Emoji.bigLayout(text, edge, max))
        }}
        // Fresh row, props as JSON. Big: each picture 'x,y,edge', label text, label height,
        // body width. Else inline: font px, picture edge in text ('-' if none).
        function body(url, json) {{
            var props = JSON.parse(json)
            props.width = 540
            delegate.source = ''
            delegate.setSource(url, props)
            if (delegate.status !== Loader.Ready) {{ return 'load-failed' }}
            var label = findIn(delegate.item, 'messageLabel')
            if (!label) {{ return 'missing:messageLabel' }}
            var bigs = []
            for (var i = 0; i < label.children.length; i++) {{
                var kid = label.children[i]
                if (kid.objectName === 'bigEmoji') {{
                    // Picture shipped and drawn (SVG plugin there), not just placed.
                    if (kid.status !== Image.Ready) {{ return 'not-loaded:' + kid.source }}
                    bigs.push(Math.round(kid.x) + ',' + Math.round(kid.y) + ',' + kid.width)
                }}
            }}
            if (bigs.length > 0) {{
                return 'big:' + bigs.join(';') + ':' + label.text + ':'
                       + Math.round(label.height) + ':' + Math.round(delegate.item.bodyWidth)
            }}
            var edge = /width=.(\d+)/.exec(label.text)
            return 'inline:' + label.font.pixelSize + ':' + (edge ? edge[1] : '-')
        }}
    }}
",
        qml = qml.display()
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn few_emoji_alone_are_drawn_big() {
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

    // Stub fontSizeMedium 18: edge round(18 x 2.875) = 52, gap 4, sender's space 13, line 56.
    // Inline: "normal", label's own size, picture 1.15 x it.
    let rows: Vec<(&'static str, Value, &'static str)> = vec![
        // Bubble exactly as wide as pictures: 52 + 4 + 52.
        (
            "big",
            json!({ "messageText": "👏👏" }),
            "big:0,0,52;56,0,52::52:108",
        ),
        (
            "big-spaced",
            json!({ "messageText": "👏 👏" }),
            "big:0,0,52;69,0,52::52:121",
        ),
        // Shim's rendering joins lines with `<br>`: judged on text as sent. Narrower than
        // bubble's least width (itemSizeSmall 60).
        (
            "big-styled",
            json!({ "markdownMode": 0, "messageText": "🔥\n🔥", "styledText": "🔥<br>🔥" }),
            "big:0,0,52;0,56,52::108:60",
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

    // Edge 100: gap 8, space 25, line 108. Wraps past width given; spaces and breaks as sent.
    let layouts: Vec<(&'static str, &'static str, f64, &'static str)> = vec![
        (
            "wrap",
            "😀😀😀",
            250.0,
            r#"{"pictures":[{"picture":"1f600","x":0,"y":0},{"picture":"1f600","x":108,"y":0},{"picture":"1f600","x":0,"y":108}],"width":208,"height":208}"#,
        ),
        (
            "spaces",
            " 😀  😀\n\n😀 ",
            1000.0,
            r#"{"pictures":[{"picture":"1f600","x":0,"y":0},{"picture":"1f600","x":158,"y":0},{"picture":"1f600","x":0,"y":216}],"width":258,"height":316}"#,
        ),
    ];
    let layouts_ptr = std::ptr::addr_of!(layouts);

    single_shot(Duration::from_secs(1), move || unsafe {
        for (label, text, _) in &*texts_ptr {
            (*steps_ptr).push((
                (*label).to_string(),
                call!("onlyEmoji", QString::from(*text)),
            ));
        }
        for (label, text, width, _) in &*layouts_ptr {
            (*steps_ptr).push((
                (*label).to_string(),
                call!("layout", QString::from(*text), 100.0, *width),
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
    for (label, text, _, expected) in &layouts {
        assert_eq!(
            common::value_of(&steps, label),
            *expected,
            "{label}: {text:?} laid out big. {context}"
        );
    }
    // Label's own size, from row without emoji.
    let normal = common::value_of(&steps, "text")
        .strip_prefix("inline:")
        .and_then(|rest| rest.split_once(':'))
        .and_then(|(px, _)| px.parse::<f64>().ok())
        .unwrap_or(0.0);
    assert!(
        normal > 0.0,
        "plain text row's font is {normal} px. {context}"
    );
    for (label, props, expected) in &rows {
        let wanted = if *expected == "normal" {
            let has_emoji = props["messageText"] != "hi there";
            #[allow(clippy::cast_possible_truncation)]
            let edge = (normal * 1.15).round() as i64;
            format!(
                "inline:{normal}:{}",
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
            "{label}: big pictures, or inline font px / picture edge. {context}"
        );
    }
}
