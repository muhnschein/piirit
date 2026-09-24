//! A line break in a message is drawn as a line break.
//!
//! The bubble draws the shim's rendering of a body as `Text.StyledText`,
//! and in that format a newline is *whitespace*: a body joined with
//! newlines comes out as one running paragraph however it was typed. Two
//! whole features were quietly broken by it -- a to-do list somebody
//! sent arrived as a sentence, and the fold never offered to open it out,
//! because a message that Qt lays out as one line has nothing to
//! truncate.
//!
//! So this measures what Qt does rather than trusting a reading of it:
//! each shape the renderer can emit goes through a `Text` in the same
//! format the bubble uses, and the lines it comes out as are counted.
//! `markdown.rs` has the other half -- that the renderer emits these
//! shapes -- and the two together are the whole chain.

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

/// Forty lines, rendered the way the shim renders them.
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

    let long = a_long_styled_body();

    single_shot(Duration::from_secs(1), move || unsafe {
        // A body joined with newlines, and the same body joined with `<br>`.
        record!("newlines", call!("lines", QString::from("one\ntwo\nthree")));
        record!(
            "breaks",
            call!("lines", QString::from("one<br>two<br>three"))
        );
        // A blank line between two paragraphs.
        record!("blank", call!("lines", QString::from("one<br><br>three")));
        // A code block: `<pre>` keeps the newlines in it, and its own
        // tags start and end a line -- which is why the renderer puts no
        // `<br>` at either edge of one.
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

    single_shot(Duration::from_secs(2), move || unsafe {
        // A forty-line body, drawn from the rendering: the fold has to
        // see it as forty lines rather than one.
        call!("set", QString::from("markdownMode"), 0);
        call!(
            "set",
            QString::from("messageText"),
            QString::from("the 1st thing to do today")
        );
        call!(
            "set",
            QString::from("styledText"),
            QString::from(long.clone())
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
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

    // The measurement the fix rests on. If this ever comes back as 3,
    // Qt has started treating a newline as a break and `join_styled`
    // could be simpler -- but nothing is broken by the `<br>`.
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
    // a / x / y / b, with the block's own tags doing the breaking.
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
