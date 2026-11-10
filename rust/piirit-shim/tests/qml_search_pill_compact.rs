//! Search pill gives up its word, keeps magnifier, once worded pills would run past right edge.
//! Decided from widths, not language or screen: words, text size, page width all count.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::log::{install_message_handler, QMessageLogContext, QtMsgType};
use qmetaobject::*;
use std::sync::Mutex;

mod common;

/// Binding loops QML only logs: width deciding compact deciding width would flicker on phone.
static LOOPS: Mutex<Vec<String>> = Mutex::new(Vec::new());

extern "C" fn catch_loops(_: QtMsgType, _: &QMessageLogContext, message: &QString) {
    let text = message.to_string();
    eprintln!("{text}");
    if text.contains("Binding loop") {
        LOOPS.lock().expect("lock").push(text);
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url, accountId, archived) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId, archived: archived })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
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
            // A row's menu is not among its children.
            if (node.menu) {
                var inMenu = findIn(node.menu, name)
                if (inMenu) { return inMenu }
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function setWidth(width) { loader.item.width = width; return 'ok' }
        // Width five worded pills take, margins both sides: read off the pills themselves.
        function need() {
            var row = findIn(loader.item, 'chatListPillRow')
            var names = ['chatSearchPill', 'unreadFilterButton', 'chatsFilterButton',
                         'channelsFilterButton', 'archivedFilterButton']
            var total = 2 * row.x + (names.length - 1) * row.spacing
            for (var i = 0; i < names.length; i++) {
                var pill = findIn(loader.item, names[i])
                total += pill.fullWidth
            }
            return '' + total
        }
        // compact, word shown, pill narrower than worded, content padded right (worded = word
        // then paddingLarge, compact = icon centred), round, field's magnifier on pill's.
        function search() {
            var pill = findIn(loader.item, 'chatSearchPill')
            var label = findIn(pill, 'pillLabel')
            var content = label.parent
            // Row's own width waits for a polish pass, which a windowless test never runs.
            var used = -content.spacing
            for (var i = 0; i < content.children.length; i++) {
                var kid = content.children[i]
                if (kid.visible) { used += kid.width + content.spacing }
            }
            var right = pill.width - content.x - used
            // Field's magnifier where pill's is: cross-fade must not jump.
            var pillIcon = content.children[0].mapToItem(loader.item, 0, 0)
            var fieldIcon = findIn(loader.item, 'chatSearchMagnifier').mapToItem(loader.item, 0, 0)
            var field = findIn(loader.item, 'chatSearchField')
            var aligned = Math.abs(pillIcon.x - fieldIcon.x) < 0.5
                && Math.abs(field.textLeftMargin - (fieldIcon.x + Theme.iconSizeSmallPlus
                                                    + Theme.paddingSmall)) < 0.5
            var padded = Math.abs(right - (pill.compact ? content.x : Theme.paddingLarge)) < 0.5
            return pill.compact + ' ' + label.visible + ' ' + (pill.width < pill.fullWidth)
                + ' ' + padded + ' ' + (pill.width === pill.height) + ' ' + aligned
        }
        function deleteChat(chatId) {
            var chats = findIn(loader.item, 'chats')
            if (!chats) { return 'missing:chats' }
            chats.delete_chat(chatId)
            return 'ok'
        }
    }
";

/// `search()`: compact, word shown, narrower than worded, padded right, round, field's magnifier
/// on pill's.
const WORDED: &str = "false true false true false true";
const COMPACT: &str = "true false true true true true";

#[test]
#[allow(clippy::too_many_lines)]
fn the_search_pill_shrinks_to_its_icon_only_when_the_pills_would_not_fit() {
    let temp = std::env::temp_dir().join(format!("piirit-search-compact-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server start.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Chat 2 a channel: all four pills.
        std::env::set_var("PIIRIT_FAKE_CHANNELS", "2");
    }

    install_message_handler(Some(catch_loops));
    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let mut need = 0.0_f64;
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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }
    let need_ptr: *mut f64 = std::ptr::addr_of_mut!(need);
    // Record once `search` says `$want` (last try: as it is), then `$then`.
    macro_rules! once {
        ($label:expr, $want:expr, $then:expr) => {
            move |last| unsafe {
                let now = call!("search");
                if now != $want && !last {
                    return false;
                }
                record!($label, now);
                $then;
                true
            }
        };
    }

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!(
            "loaded",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1,
                false
            )
        );
    });

    plan.until(1, 30, move |last| unsafe {
        if get!("channelsFilterButton", "visible") != "true" && !last {
            return false;
        }
        *need_ptr = call!("need").parse().unwrap_or_default();
        call!("setWidth", *need_ptr);
        true
    });
    plan.until(
        1,
        30,
        once!("exact", WORDED, call!("setWidth", *need_ptr - 1.0)),
    );
    plan.until(
        1,
        30,
        once!("one-short", COMPACT, call!("setWidth", 10_000.0_f64)),
    );
    plan.until(
        1,
        30,
        once!("wide", WORDED, call!("setWidth", *need_ptr - 1.0)),
    );
    // Last channel gone: three pills fit where five did not, word comes back.
    plan.until(1, 30, once!("narrow", COMPACT, call!("deleteChat", 2u32)));
    plan.until(1, 30, move |last| unsafe {
        if get!("channelsFilterButton", "visible") != "false" && !last {
            return false;
        }
        true
    });
    plan.until(1, 30, once!("two-pills", WORDED, (*engine_ptr).quit()));

    plan.run();
    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("loaded"),
        "ok",
        "the chat list did not load. {context}"
    );
    assert!(need > 0.0, "the pills have no width to fit. {context}");
    assert_eq!(
        value("exact"),
        WORDED,
        "with room for every worded pill, Search lost its word. {context}"
    );
    assert_eq!(
        value("one-short"),
        COMPACT,
        "one pixel short, Search kept its word and pushed Channels past the edge. {context}"
    );
    assert_eq!(
        value("wide"),
        WORDED,
        "on a wide page Search did not get its word back. {context}"
    );
    assert_eq!(
        value("narrow"),
        COMPACT,
        "Search took its word back on a narrow page. {context}"
    );
    assert_eq!(
        value("two-pills"),
        WORDED,
        "with only Search, Unread and Archived left, which fit, Search stayed an icon. {context}"
    );

    let loops = LOOPS.lock().expect("lock").clone();
    assert!(
        loops.is_empty(),
        "Search's width fed back into whether it fits: {loops:?}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}
