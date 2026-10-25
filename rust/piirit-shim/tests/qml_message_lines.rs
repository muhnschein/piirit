//! Line break drawn as line break. Bubble's `Text.StyledText` treats newline as
//! whitespace: newline-joined body became one paragraph. Broke to-do lists (arrived as a
//! sentence) and fold (one laid-out line has nothing to truncate). Measures Qt directly: each
//! renderer shape through same-format `Text`, lines counted. `markdown.rs` pins other half
//! (renderer emits these shapes); together, whole chain.

// Qt harness: see qml_reactions.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540

        // The body as MessageDelegate draws it: wrapped, in the format
        // the shim's rendering is meant for.
        Text {
            id: styled
            width: 500
            wrapMode: Text.Wrap
            textFormat: Text.StyledText
        }
        function lines(body) {
            styled.text = body
            return '' + styled.lineCount
        }

        // And the delegate itself, to see the fold notice a body it is
        // not showing whole.
        Loader { id: delegate }
        function loadDelegate(url) {
            delegate.setSource(url, { width: 540 })
            return delegate.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            delegate.item[property] = value
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
        function get(name, property) {
            var item = findIn(delegate.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
    }
";

/// Forty lines, rendered as shim renders them.
fn a_long_styled_body() -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for number in 1..=40 {
        if number > 1 {
            out.push_str("<br>");
        }
        let _ = write!(out, "the {number}th thing to do today");
    }
    out
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_renderings_line_breaks_are_line_breaks_on_the_screen() {
    // SAFETY: single thread; pre-Qt.
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

    let long = a_long_styled_body();

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        // Newline-joined body and `<br>`-joined body.
        record!("newlines", call!("lines", QString::from("one\ntwo\nthree")));
        record!(
            "breaks",
            call!("lines", QString::from("one<br>two<br>three"))
        );
        // Blank line between paragraphs.
        record!("blank", call!("lines", QString::from("one<br><br>three")));
        // Code block: `<pre>` keeps newlines; its tags start and end a line, so no `<br>` at edges.
        record!("block", call!("lines", QString::from("a<pre>x\ny</pre>b")));
        record!(
            "block-with-breaks",
            call!("lines", QString::from("a<br><pre>x\ny</pre><br>b"))
        );
        record!(
            "load",
            call!(
                "loadDelegate",
                QString::from(common::component_url("MessageDelegate.qml"))
            )
        );
    });

    plan.after(1, move || unsafe {
        // Forty-line body from rendering: fold must see forty lines.
        call!("set", QString::from("markdownMode"), 0);
        call!(
            "set",
            QString::from("messageText"),
            QString::from("the 1st thing to do today")
        );
        call!("set", QString::from("styledText"), QString::from(long));
    });

    plan.after(1, move || unsafe {
        record!(
            "styled-truncated",
            call!(
                "get",
                QString::from("messageLabel"),
                QString::from("truncated")
            )
        );
        record!(
            "styled-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        record!(
            "styled-lines",
            call!(
                "get",
                QString::from("messageLabel"),
                QString::from("lineCount")
            )
        );
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

    // Measurement fix rests on. If it returns 3, Qt breaks on newline and `join_styled`
    // could be simpler; `<br>` harmless.
    assert_eq!(
        value("newlines"),
        "1",
        "a newline is being drawn as a line break in StyledText, which \
         is not what this Qt did. {context}"
    );
    assert_eq!(
        value("breaks"),
        "3",
        "three lines joined with <br> were not drawn as three lines, so \
         nothing the renderer emits will break where it was typed. \
         {context}"
    );
    assert_eq!(
        value("blank"),
        "3",
        "a blank line between two paragraphs was collapsed. {context}"
    );
    // a / x / y / b; block's own tags do the breaking.
    assert_eq!(
        value("block"),
        "4",
        "a code block did not lay out as its own lines between the two \
         around it. {context}"
    );
    assert!(
        number("block-with-breaks") > number("block"),
        "a <br> at the edge of a code block cost nothing, so the \
         renderer need not avoid one -- it was avoided because it left a \
         blank line behind. {context}"
    );

    assert_eq!(value("load"), "ok", "the delegate did not load. {context}");
    let lines = number("styled-lines");
    assert!(
        lines > 1.0,
        "a forty-line rendering was drawn as {lines} line(s) in the \
         bubble: every line break in the message is gone. {context}"
    );
    assert_eq!(
        value("styled-truncated"),
        "true",
        "the bubble thinks it is showing the whole of a forty-line \
         message. {context}"
    );
    assert_eq!(
        value("styled-actions"),
        "true",
        "a forty-line rendering offered no way to the rest of it -- \
         which is what a body Qt lays out as one line does. {context}"
    );
}
