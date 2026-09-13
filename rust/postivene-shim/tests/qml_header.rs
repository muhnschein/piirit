//! The conversation header is laid out as Silica's `PageHeader` lays out
//! its own.
//!
//! The title on the line the page indicator sits on, right-aligned, as
//! wide as its text and no wider than the page less its margins, and in
//! the page's own colour once the header leads somewhere.
//!
//! And the second line under it, where `PageHeader` puts its description:
//! what a group's header says about itself. The header keeps its height
//! whatever is in it, so the line is drawn only while both fit inside
//! that height -- otherwise it would reach over the first message.

// Qt harness: see qml_chat_list.rs.
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
    import Sailfish.Silica 1.0
    Item {
        width: 1080
        Loader { id: loader; width: 1080 }
        function load(url, title) {
            loader.setSource('', {})
            loader.setSource(url, { title: title })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
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
        // Header height, the title's width, its left edge, and whether it
        // is drawn in the page's colour.
        function layout() {
            var label = findIn(loader.item, 'headerTitle')
            if (!label) { return 'missing:headerTitle' }
            return loader.item.height + ',' + label.width + ',' + label.x + ','
                + (label.color == Theme.primaryColor ? 'primary' : 'highlight')
        }
        function setInteractive(on) {
            loader.item.interactive = on
            return 'ok'
        }
        function setSubtitle(text) {
            loader.item.subtitle = text
            return 'ok'
        }
        // The header made too short for two lines. A binding broken on
        // purpose: the header is PageHeader-tall in the app, and this is
        // the reader whose fonts fill that with the name alone.
        function setHeight(height) {
            loader.item.height = height
            return 'ok'
        }
        // Whether the second line is drawn, its width and left edge, the
        // gap between it and the title, and the room left above the
        // title and below the line -- which are equal when the pair is
        // centred where the title alone would be.
        function secondLine() {
            var title = findIn(loader.item, 'headerTitle')
            var line = findIn(loader.item, 'headerSubtitle')
            if (!title || !line) { return 'missing:headerSubtitle' }
            return (line.visible ? 'shown' : 'hidden') + ',' + line.width + ','
                + line.x + ',' + (line.y - (title.y + title.height)) + ','
                + title.y + ','
                + (loader.item.height - (line.y + line.height)) + ','
                + (line.color == Theme.secondaryColor ? 'secondary' : 'other')
        }
    }
";

#[test]
fn the_header_is_laid_out_as_a_page_header() {
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

    let header = common::component_url("ConversationHeader.qml");
    let long = "a very long name that would run the whole way across the header and further";
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(header.clone()), QString::from(long))
        );
        record!("long", call!("layout"));
        // A short title is not widened to the room it has.
        record!(
            "load-short",
            call!("load", QString::from(header.clone()), QString::from("Ada"))
        );
        record!("short", call!("layout"));
        call!("setInteractive", true);
        record!("leads", call!("layout"));
        // The line a group's header carries under its name.
        record!("subtitle", call!("setSubtitle", QString::from("7 members")));
        record!("second-line", call!("secondLine"));
        // And the same header with no room for it.
        record!("shrink", call!("setHeight", 20));
        record!("no-room", call!("secondLine"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_layout(&steps);
}

/// `PageHeader`'s height and colours, the page less its margins for a
/// long title, and a short one at its own width.
fn assert_layout(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let layout = |label: &str| -> (f64, f64, f64, String) {
        let text = value(label);
        let mut parts = text.split(',');
        let mut number = || parts.next().and_then(|n| n.parse().ok()).unwrap_or(-1.0);
        let (height, width, x) = (number(), number(), number());
        (
            height,
            width,
            x,
            parts.next().unwrap_or_default().to_string(),
        )
    };
    let close = |a: f64, b: f64| (a - b).abs() < 0.5;

    for label in ["load", "load-short"] {
        assert_eq!(value(label), "ok", "the header did not load. {context}");
    }
    // The stub theme: itemSizeLarge 120, horizontalPageMargin 24, and a
    // header that is highlight-coloured until it leads somewhere. A long
    // title reaches well past the middle, ends at the right margin, and
    // never crosses the left one.
    let (height, width, x, colour) = layout("long");
    assert!(
        close(height, 120.0)
            && width > 540.0
            && x >= 24.0 - 0.5
            && close(x + width, 1080.0 - 24.0)
            && colour == "highlight",
        "a long title is not laid out as PageHeader lays one out: \
         {height} {width} {x} {colour}. {context}"
    );
    // A short title is as wide as its text and no wider, right-aligned.
    let (_, width, x, _) = layout("short");
    assert!(
        width < 200.0 && close(x + width, 1080.0 - 24.0),
        "a short title is not right-aligned at its own width: {width} {x}. {context}"
    );
    let (_, _, _, colour) = layout("leads");
    assert_eq!(
        colour, "primary",
        "a header that leads somewhere is not drawn in the page's colour. {context}"
    );
    assert_second_line(&value("second-line"), &value("no-room"), context.as_str());
}

/// The second line: right-aligned at its own width under the title, with
/// no gap, the pair centred where the title alone sat, and in the
/// quieter of the two colours the title is drawn from. Gone entirely
/// from a header with no room for both.
fn assert_second_line(shown: &str, cramped: &str, context: &str) {
    let parts: Vec<&str> = shown.split(',').collect();
    assert_eq!(parts.len(), 7, "unexpected second line. {context}");
    let number = |at: usize| -> f64 { parts[at].parse().unwrap_or(-1.0) };
    let close = |a: f64, b: f64| (a - b).abs() < 1.5;
    let (width, x, gap, above, below) = (number(1), number(2), number(3), number(4), number(5));
    assert!(
        parts[0] == "shown"
            && width < 200.0
            && close(x + width, 1080.0 - 24.0)
            && close(gap, 0.0)
            && close(above, below)
            && parts[6] == "secondary",
        "the line under the title is not laid out as a header's second          line: {shown}. {context}"
    );
    assert!(
        cramped.starts_with("hidden"),
        "a header with no room for two lines still draws the second one,          so it reaches over what sits below it: {cramped}. {context}"
    );
}
