//! Reactions as the conversation draws them: chips hung off the bubble's
//! inside bottom corner, ours lit, and the quick-reaction row at the top
//! of a row's menu.
//!
//! The delegate and the list are loaded on their own, as the other QML
//! tests load them: the page cannot be loaded headlessly.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
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
        height: 400

        property string raised: ''

        // The delegate on its own.
        Loader { id: delegate }
        function loadDelegate(url) {
            delegate.setSource(url, { width: 540 })
            if (delegate.status !== Loader.Ready) { return 'load-failed' }
            delegate.item.reactionRequested.connect(function(emoji) {
                raised = 'chip:' + emoji
            })
            return 'ok'
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
            var item = findIn(delegate.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function chips() {
            var found = findAll(delegate.item, 'reactionChip', [])
            var parts = []
            for (var i = 0; i < found.length; i++) {
                var label = findIn(found[i], 'chipLabel')
                parts.push((label ? label.text : '?')
                           + (found[i].mine ? '*' : '')
                           + '@' + Math.round(found[i].x))
            }
            return parts.join('|')
        }
        function tapChip(index) {
            var found = findAll(delegate.item, 'reactionChip', [])
            if (index >= found.length) { return 'missing' }
            delegate.item.reactionRequested(found[index].emoji)
            return 'ok'
        }
        function height() { return '' + delegate.item.height }
        function raisedSignal() { return raised }

        // The list, over a plain model with one row in it.
        ListModel { id: rows }
        Loader { id: list; width: 540; height: 400 }
        function loadList(url) {
            rows.append({
                message_id: 7, text: 'one', is_outgoing: false,
                is_info: false, show_padlock: true, state: 16,
                timestamp: 1700000000, day_number: 19675,
                sender_name: 'Ada', sender_color: '#00875a',
                quote_text: '', quote_author: '', file_path: '',
                file_name: '', view_type: 'Text', image_width: 0,
                image_height: 0, reactions: ''
            })
            list.setSource(url, { model: rows })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.reactionRequested.connect(function(id, emoji) {
                raised = 'menu:' + id + ':' + emoji
            })
            return 'ok'
        }
        // The menu is a property of the row, not a visual child of it.
        function menuOptions() {
            var row = findIn(list.item, 'messageRow')
            if (!row || !row.menu) { return 'no-menu' }
            var found = findAll(row.menu, 'reactionOption', [])
            var parts = []
            for (var i = 0; i < found.length; i++) { parts.push(found[i].emoji) }
            return parts.join('')
        }
        function pickOption(index) {
            var row = findIn(list.item, 'messageRow')
            if (!row || !row.menu) { return 'no-menu' }
            var found = findAll(row.menu, 'reactionOption', [])
            if (index >= found.length) { return 'missing' }
            found[index].choose()
            return 'ok'
        }
        function pickerVisible() {
            var row = findIn(list.item, 'messageRow')
            if (!row || !row.menu) { return 'no-menu' }
            var picker = findIn(row.menu, 'reactionPicker')
            return picker ? '' + picker.visible : 'missing'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn reactions_are_chips_on_the_message_and_a_row_in_its_menu() {
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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "loadDelegate",
                QString::from(common::component_url("MessageDelegate.qml"))
            )
        );
        call!("set", QString::from("messageText"), QString::from("hello"));
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("bare-height", call!("height"));
        record!(
            "bare-row",
            call!(
                "get",
                QString::from("reactionRow"),
                QString::from("visible")
            )
        );
        call!(
            "set",
            QString::from("reactions"),
            QString::from(
                r#"[{"emoji":"👍","count":2,"self":true},{"emoji":"❤️","count":1,"self":false}]"#
            )
        );
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("reacted-height", call!("height"));
        record!(
            "reacted-row",
            call!(
                "get",
                QString::from("reactionRow"),
                QString::from("visible")
            )
        );
        record!("chips", call!("chips"));
        record!(
            "footer-y",
            call!("get", QString::from("footerLabel"), QString::from("y"))
        );
        record!(
            "row-y",
            call!("get", QString::from("reactionRow"), QString::from("y"))
        );
        record!(
            "row-height",
            call!("get", QString::from("reactionRow"), QString::from("height"))
        );
        record!(
            "row-x",
            call!("get", QString::from("reactionRow"), QString::from("x"))
        );
        record!(
            "row-width",
            call!("get", QString::from("reactionRow"), QString::from("width"))
        );
        record!(
            "bubble-x",
            call!("get", QString::from("bubble"), QString::from("x"))
        );
        record!(
            "bubble-width",
            call!("get", QString::from("bubble"), QString::from("width"))
        );
        record!(
            "bubble-height",
            call!("get", QString::from("bubble"), QString::from("height"))
        );
        record!("tap", call!("tapChip", 1));
        record!("chip-raised", call!("raisedSignal"));
        call!("set", QString::from("reactions"), QString::from(""));
    });
    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "cleared-row",
            call!(
                "get",
                QString::from("reactionRow"),
                QString::from("visible")
            )
        );
        record!("cleared-height", call!("height"));
        record!(
            "list",
            call!(
                "loadList",
                QString::from(common::component_url("ConversationList.qml"))
            )
        );
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("picker", call!("pickerVisible"));
        record!("options", call!("menuOptions"));
        record!("pick", call!("pickOption", 1));
        record!("menu-raised", call!("raisedSignal"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the delegate did not load. {context}");
    assert_eq!(
        value("bare-row"),
        "false",
        "a message nobody reacted to shows a reaction strip. {context}"
    );
    assert_eq!(
        value("reacted-row"),
        "true",
        "a message with reactions does not show them. {context}"
    );
    // Ours is marked, the count shows only past one, and the second chip
    // sits after the first rather than on top of it.
    let chips = value("chips");
    let parts: Vec<&str> = chips.split('|').collect();
    assert_eq!(
        parts.len(),
        2,
        "two reactions are not two chips: {chips:?}. {context}"
    );
    assert!(
        parts[0].starts_with("👍 2*@0"),
        "the first chip is not our counted thumbs-up at the start: {chips:?}. {context}"
    );
    let second_x: f64 = parts[1]
        .rsplit('@')
        .next()
        .and_then(|x| x.parse().ok())
        .unwrap_or(-1.0);
    assert!(
        parts[1].starts_with("❤️@") && second_x > 0.0,
        "the second chip is not an unmarked, uncounted heart placed after \
         the first: {chips:?}. {context}"
    );

    let bare: f64 = value("bare-height").parse().unwrap_or_default();
    let reacted: f64 = value("reacted-height").parse().unwrap_or_default();
    let cleared: f64 = value("cleared-height").parse().unwrap_or_default();
    assert!(
        bare > 0.0 && reacted > bare,
        "the row does not grow for its reactions (bare {bare}, reacted {reacted}). {context}"
    );
    // The strip hangs off the bubble: below the footer, which is the
    // bubble's last line, straddling the bubble's bottom edge, and at the
    // inside corner -- the right-hand one for a message from someone
    // else, whose bubble sits on the left.
    let number = |label: &str| value(label).parse::<f64>().unwrap_or(-1.0);
    let (row_y, row_height) = (number("row-y"), number("row-height"));
    let (row_x, row_width) = (number("row-x"), number("row-width"));
    let footer_y = number("footer-y");
    let (bubble_x, bubble_width, bubble_height) = (
        number("bubble-x"),
        number("bubble-width"),
        number("bubble-height"),
    );
    assert!(
        row_y > footer_y && row_y < bubble_height && row_y + row_height > bubble_height,
        "the strip does not straddle the bubble's bottom edge below the footer \
         (strip {row_y}+{row_height}, footer {footer_y}, bubble {bubble_height}). {context}"
    );
    // Inside the bubble, ending at its right-hand padding rather than
    // starting at its left-hand one.
    let bubble_right = bubble_x + bubble_width;
    let row_right = row_x + row_width;
    assert!(
        row_x > bubble_x && row_right < bubble_right && bubble_right - row_right < 16.0,
        "the strip is not at the bubble's inside corner for an incoming message \
         (strip {row_x}..{row_right}, bubble {bubble_x}..{bubble_right}). {context}"
    );
    assert_eq!(
        value("chip-raised"),
        "chip:❤️",
        "tapping a chip did not name its emoji. {context}"
    );
    assert_eq!(
        value("cleared-row"),
        "false",
        "the strip stays once the reactions are gone. {context}"
    );
    assert!(
        (cleared - bare).abs() < 0.5,
        "the row did not shrink back once the reactions were gone \
         (bare {bare}, cleared {cleared}). {context}"
    );

    assert_eq!(value("list"), "ok", "the list did not load. {context}");
    assert_eq!(
        value("picker"),
        "true",
        "the row's menu offers no reactions. {context}"
    );
    assert_eq!(
        value("options"),
        "👍❤️😂😮😢🙏",
        "the quick reactions are not the six the reference clients offer. {context}"
    );
    assert_eq!(
        value("menu-raised"),
        "menu:7:❤️",
        "picking from the menu did not name the message and the emoji. {context}"
    );
}
