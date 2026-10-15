//! Search row stays on screen and has height.
//! Width loop (title/time label) ran long names off edge. Deleted `Avatar` left height 0.
//! Unresolved ids only warn; zero-height row passes other checks. Hence geometry test.

// Qt harness: see qml_conversation.rs.
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

const ROW_WIDTH: i32 = 540;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url, width) {
            loader.setSource(url, { width: width })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
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
        /// How far the drawn text reaches past the space it was given.
        /// The item's own width is not the measure: a Text with no elide
        /// and no wrap keeps the width it was assigned and paints
        /// straight over the edge anyway, which is exactly the bug.
        function overhang(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + Math.round(item.paintedWidth - item.width)
        }
        function widthOf(name) {
            var item = findIn(loader.item, name)
            return item ? '' + Math.round(item.width) : 'missing:' + name
        }
        function heightOf(name) {
            var item = findIn(loader.item, name)
            return item ? '' + Math.round(item.height) : 'missing:' + name
        }
        function xOf(name) {
            var item = findIn(loader.item, name)
            return item ? '' + Math.round(item.x) : 'missing:' + name
        }
        /// The row itself, which is what a list asks for its row height.
        function rowHeight() {
            return '' + Math.round(loader.item.height)
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_long_result_stays_inside_the_screen() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("TZ", "UTC");
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

    // Too long to fit, with a time label: title must make room.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("SearchResultRow.qml")),
                ROW_WIDTH
            )
        );
        call!(
            "set",
            QString::from("title"),
            QString::from(
                "A chat with a truly preposterous name that nobody would ever \
                 type but which the core will happily hand back anyway"
            )
        );
        call!(
            "set",
            QString::from("subtitle"),
            QString::from(
                "and a matching message whose text runs on well past the \
                 width of any telephone screen, several times over, with no \
                 convenient place to break it"
            )
        );
        call!("set", QString::from("timestamp"), 1_700_000_000);
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "title-overhang",
            call!("overhang", QString::from("resultTitle"))
        );
        record!(
            "subtitle-overhang",
            call!("overhang", QString::from("resultSubtitle"))
        );
        record!(
            "title-width",
            call!("widthOf", QString::from("resultTitle"))
        );
        record!(
            "subtitle-height",
            call!("heightOf", QString::from("resultSubtitle"))
        );
        record!("row-height", call!("rowHeight"));
        record!("avatar-x", call!("xOf", QString::from("resultAvatar")));
        record!(
            "avatar-width",
            call!("widthOf", QString::from("resultAvatar"))
        );
        record!("title-x", call!("xOf", QString::from("resultTitle")));
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
    let number = |label: &str| value(label).parse::<i32>().unwrap_or(i32::MAX);
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the row did not load. {context}");

    // Bug: unresolved widths left title past row.
    assert!(
        number("title-overhang") <= 0,
        "the title is drawn wider than the space it was given, so it runs \
         off the right-hand edge. {context}"
    );
    assert!(
        number("subtitle-overhang") <= 0,
        "the message text is drawn wider than the space it was given, so it \
         runs off the right-hand edge. {context}"
    );
    // Nor resolve to nothing: hides row, not fixes it.
    assert!(
        number("title-width") > ROW_WIDTH / 4,
        "the title resolved to almost no width, so the row shows nothing. \
         {context}"
    );
    // Subtitle wraps: one line can't tell hits apart.
    assert!(
        number("subtitle-height") > 12,
        "the message text did not wrap to a second line. {context}"
    );

    // Avatar: height and title x measured off it; delete collapsed rows.
    // `number` reads missing item as i32::MAX: checked by name first.
    for label in ["avatar-x", "avatar-width", "title-x"] {
        assert!(
            !value(label).starts_with("missing"),
            "{label} is not there at all, so the row lost the element it \
             measures itself against. {context}"
        );
    }
    assert!(
        number("row-height") > 0,
        "the row has no height, so a list of these shows its headings and \
         nothing else. {context}"
    );
    assert!(
        number("avatar-width") > 0,
        "the row has no avatar; the height and the title's x are measured \
         off it, and without it neither binding resolves. {context}"
    );
    assert!(
        number("title-x") >= number("avatar-x") + number("avatar-width"),
        "the title starts on top of the avatar rather than beside it. \
         {context}"
    );
    assert!(
        number("row-height") > number("avatar-width"),
        "the row is shorter than the avatar in it, so the avatar is clipped \
         away. {context}"
    );
}
