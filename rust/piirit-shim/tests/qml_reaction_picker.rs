//! The reaction picker: any emoji as a reaction, drawn as a picture.
//!
//! The page on its own, as the other page tests load theirs, with
//! `pageStack` recorded rather than performed; and the one emoji
//! component every reaction is drawn with, which falls back to text only
//! for an emoji no picture is shipped for.

// Qt harness: see qml_pages.rs.
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
        height: 960

        property QtObject pageStack: QtObject {
            property string log: ''
            function pop() { log = log + 'pop|' }
        }
        property string raised: ''

        Loader { id: loader; width: 540; height: 960 }
        function load(url) {
            loader.setSource(url, {
                current: '❤',
                recent: ['🔥', '❤️', 'not an emoji']
            })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.picked.connect(function(emoji) {
                raised = raised + emoji + '|'
            })
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
        function findAll(node, name, out) {
            if (!node) { return out }
            if (node.objectName === name) { out.push(node) }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                findAll(kids[i], name, out)
            }
            return out
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function page(property) { return '' + loader.item[property] }
        // The first rows as the grid holds them: a heading, or the
        // emoji of a row of cells.
        function rows(count) {
            var rows = loader.item.rows
            var parts = []
            for (var i = 0; i < rows.length && i < count; i++) {
                if (rows[i].heading.length > 0) {
                    parts.push('#' + rows[i].heading)
                } else {
                    var cells = []
                    for (var c = 0; c < rows[i].cells.length; c++) {
                        cells.push(rows[i].cells[c][0])
                    }
                    parts.push(cells.join(''))
                }
            }
            return parts.join('|')
        }
        function rowCount() { return '' + loader.item.rows.length }
        // Every emoji the grid has drawn so far, the reader.s own marked.
        function options() {
            var found = findAll(loader.item, 'emojiOption', [])
            var parts = []
            for (var i = 0; i < found.length; i++) {
                parts.push(found[i].emoji + (found[i].mine ? '*' : ''))
            }
            return parts.join('|')
        }
        function tap(emoji) {
            var found = findAll(loader.item, 'emojiOption', [])
            for (var i = 0; i < found.length; i++) {
                if (found[i].emoji === emoji) {
                    found[i].choose()
                    return 'ok'
                }
            }
            return 'missing'
        }
        function search(text) {
            var field = findIn(loader.item, 'reactionSearchField')
            if (!field) { return 'missing' }
            field.text = text
            return 'ok'
        }
        function jump(group) {
            var found = findAll(loader.item, 'groupButton', [])
            for (var i = 0; i < found.length; i++) {
                if (found[i].group === group) {
                    found[i].clicked()
                    return 'ok'
                }
            }
            return 'missing'
        }
        function results() {
            var list = loader.item.results
            var parts = []
            for (var i = 0; i < list.length; i++) { parts.push(list[i][0]) }
            return parts.join('')
        }
        function navigation() { return pageStack.log }
        function raisedSignal() { return raised }

        // The emoji component on its own, once per emoji asked about.
        Loader { id: glyph }
        function loadGlyph(url, emoji) {
            glyph.setSource(url, { emoji: emoji, size: 40 })
            if (glyph.status !== Loader.Ready) { return 'load-failed' }
            return 'ok'
        }
        // What the component shows: the picture's file, or the text.
        function glyphShows() {
            var image = findIn(glyph.item, 'emojiImage')
            var text = findIn(glyph.item, 'emojiText')
            if (image && image.visible) {
                return 'picture:' + ('' + image.source).split('/').pop()
            }
            if (text && text.visible) {
                return 'text:' + text.text
            }
            return 'nothing'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_picker_offers_every_emoji_and_reports_the_one_picked() {
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
    macro_rules! glyph {
        ($label:expr, $emoji:expr) => {{
            call!(
                "loadGlyph",
                QString::from(common::component_url("EmojiGlyph.qml")),
                QString::from($emoji)
            );
            record!($label, call!("glyphShows"));
        }};
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("ReactionPickerPage.qml"))
            )
        );
        glyph!("glyph-thumbs", "👍");
        glyph!("glyph-bare-heart", "\u{2764}");
        glyph!("glyph-toned", "👍🏽");
        glyph!("glyph-text", "hi");
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("columns", call!("page", QString::from("columns")));
        record!("rows", call!("rows", 4));
        record!("row-count", call!("rowCount"));
        record!("options", call!("options"));
        record!(
            "strip",
            call!("get", QString::from("groupStrip"), QString::from("visible"))
        );
        record!("jump", call!("jump", 8));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("top-group", call!("page", QString::from("topGroup")));
        record!(
            "scrolled",
            call!("get", QString::from("emojiGrid"), QString::from("contentY"))
        );
        record!("search", call!("search", QString::from("hot pepp")));
    });
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("found", call!("results"));
        record!(
            "strip-searching",
            call!("get", QString::from("groupStrip"), QString::from("visible"))
        );
        record!(
            "placeholder-found",
            call!(
                "get",
                QString::from("noEmojiPlaceholder"),
                QString::from("enabled")
            )
        );
        call!("search", QString::from("qqqqzz"));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        record!(
            "placeholder-none",
            call!(
                "get",
                QString::from("noEmojiPlaceholder"),
                QString::from("enabled")
            )
        );
        call!("search", QString::from(""));
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("tap-fire", call!("tap", QString::from("🔥")));
        record!("tap-mine", call!("tap", QString::from("\u{2764}\u{fe0f}")));
        record!("raised", call!("raisedSignal"));
        record!("navigation", call!("navigation"));
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

    assert_eq!(value("load"), "ok", "the picker did not load. {context}");

    // Pictures wherever one is shipped, the same one whichever way the
    // emoji is spelled, and the text only for what has none.
    assert_eq!(
        value("glyph-thumbs"),
        "picture:1f44d.png",
        "a thumbs-up is not drawn as its picture. {context}"
    );
    assert_eq!(
        value("glyph-bare-heart"),
        "picture:2764.png",
        "a heart sent without its variation selector is not drawn as the heart. {context}"
    );
    assert_eq!(
        value("glyph-toned"),
        "picture:1f44d.png",
        "a toned thumbs-up is not drawn as the untoned picture. {context}"
    );
    assert_eq!(
        value("glyph-text"),
        "text:hi",
        "what is not an emoji is not drawn as the text it is. {context}"
    );

    // 540 wide over 90-wide medium items in the stubs: room around
    // each emoji, not a grid packed edge to edge.
    assert_eq!(value("columns"), "6", "{context}");
    // The recent ones first, without what has no picture, then the
    // groups in Unicode's order.
    assert_eq!(
        value("rows"),
        "#Recent|🔥❤️|#Smileys & emotion|😀😃😄😁😆😅",
        "the grid does not open on the recent emoji and then the smileys. {context}"
    );
    let row_count: usize = value("row-count").parse().unwrap_or_default();
    assert!(
        row_count > 200,
        "the grid has {row_count} rows; six columns of every emoji is over two hundred. {context}"
    );
    let options = value("options");
    assert!(
        options.contains("🔥|❤️*") && options.contains("😀"),
        "the grid does not draw the recent emoji with the reader's own marked, \
         or the smileys after them: {options:?}. {context}"
    );
    assert!(
        options
            .split('|')
            .filter(|option| option.ends_with('*'))
            .all(|option| option == "❤️*"),
        "something other than the reader's own reaction is marked: {options:?}. {context}"
    );
    assert_eq!(value("strip"), "true", "{context}");
    assert_eq!(
        value("jump"),
        "ok",
        "there is no button for the flags. {context}"
    );
    assert_eq!(
        value("top-group"),
        "8",
        "jumping to the flags does not light their button. {context}"
    );
    let scrolled: f64 = value("scrolled").parse().unwrap_or_default();
    assert!(
        scrolled > 0.0,
        "jumping to the flags did not scroll the grid. {context}"
    );

    assert!(
        value("found").contains("🌶"),
        "searching for the start of two words does not find the hot pepper: {:?}. {context}",
        value("found")
    );
    assert_eq!(
        value("strip-searching"),
        "false",
        "the group buttons stay up while there are no groups to jump between. {context}"
    );
    assert_eq!(value("placeholder-found"), "false", "{context}");
    assert_eq!(
        value("placeholder-none"),
        "true",
        "a search that finds nothing does not say so. {context}"
    );

    // A pick is reported and the page goes; the reader's own is reported
    // spelled the way it was sent, so the core takes it off.
    assert_eq!(value("tap-fire"), "ok", "{context}");
    assert_eq!(value("tap-mine"), "ok", "{context}");
    assert_eq!(
        value("raised"),
        "🔥|\u{2764}|",
        "the picks were not reported, or the reader's own was not given back \
         as it was sent. {context}"
    );
    assert_eq!(
        value("navigation"),
        "pop|pop|",
        "picking did not go back to the conversation. {context}"
    );
}
