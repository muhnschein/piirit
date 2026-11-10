//! Chat's notifications page and Notifications row.
//!
//! Page: "on" first, then reference clients' lengths, as Delta Chat words them. Active choice
//! marked, with time left; muted with end not known: none marked. Tap marks, saves nothing;
//! swipe back saves once, only another choice. Note on mentions only when caller says they
//! still notify. Row: length and time left, time left alone when narrow. Callers:
//! `qml_mute_flow.rs`.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Choices in order, as `seconds`.
const CHOICES: [i32; 6] = [0, 3600, 28800, 86400, 604_800, -1];

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    Item {
        id: probe
        property var picked: []
        property var choices: []
        Loader { id: loader }
        Loader { id: rowLoader }
        /// Chat 2, as its group page holds it.
        ChatInfo { id: chat; objectName: 'chat'; account_id: 1; chat_id: 2 }
        Connections {
            target: typeof core === 'undefined' ? null : core
            ignoreUnknownSignals: true
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        function setChoices(list) { choices = list }
        function load(url, chatId, muted, mentions) {
            picked = []
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: 1, chatId: chatId, chatName: 'Hikers', muted: muted,
                mentionsStillNotify: mentions
            })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.picked.connect(function(seconds) { picked.push(seconds) })
            return 'ok'
        }
        function loadRow(url, width) {
            rowLoader.setSource('', {})
            rowLoader.setSource(url, {
                accountId: 1, chatId: 2, chatName: 'Hikers', chat: chat, width: width
            })
            if (rowLoader.status !== Loader.Ready) { return 'load-failed' }
            // As the group page binds it.
            rowLoader.item.muted = Qt.binding(function() { return chat.is_muted })
            return 'ok'
        }
        function rowValue() { return rowLoader.item ? rowLoader.item.value : 'missing' }
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
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        /// Swipe back: Deactivating is what saves.
        function leave() {
            loader.item.status = PageStatus.Active
            loader.item.status = PageStatus.Deactivating
            return picked.join(',')
        }
        /// Each choice as `seconds=checked`, in the order shown.
        function options() {
            var out = []
            for (var i = 0; i < choices.length; i++) {
                out.push(choices[i] + '=' + get('muteOption' + choices[i], 'checked'))
            }
            return out.join(',')
        }
        /// Choices as the page lays them out, top to bottom.
        function shownOrder() {
            var walk = function(node) {
                var name = '' + node.objectName
                var out = name.indexOf('muteOption') === 0 ? [name.substring(10)] : []
                var kids = node.data !== undefined ? node.data : node.children
                for (var i = 0; kids && i < kids.length; i++) { out = out.concat(walk(kids[i])) }
                return out
            }
            return walk(loader.item).join(',')
        }
        function labels() {
            var out = []
            for (var i = 0; i < choices.length; i++) {
                out.push(get('muteOption' + choices[i], 'text'))
            }
            return out.join('|')
        }
        function mute(seconds) { chat.set_mute_duration(seconds); return 'ok' }
        function reloadChat() { chat.reload(); return 'ok' }
        function chatState() { return chat.loaded + ',' + chat.is_muted }
    }
";

fn choices() -> QVariant {
    let list: QVariantList = CHOICES
        .iter()
        .map(|&seconds| QVariant::from(seconds))
        .collect();
    list.into()
}

fn url() -> QVariant {
    QString::from(common::page_url("ChatNotificationsPage.qml")).into()
}

/// `seconds=checked` for each choice, `marked` alone checked (None: none).
fn marked(marked: Option<i32>) -> String {
    CHOICES
        .iter()
        .map(|&seconds| format!("{seconds}={}", Some(seconds) == marked))
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_page_offers_the_choices_and_saves_another_one_on_leaving() {
    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    piirit_shim::register_qml_types();

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    // Each step synchronous: no event loop needed. No core: kept mute never read.
    let mut call = |name: &str, args: &[QVariant]| -> String {
        QString::from_qvariant(engine.invoke_method(name.into(), args))
            .map(|value| value.to_string())
            .unwrap_or_default()
    };
    let named = |name: &str| -> QVariant { QString::from(name).into() };
    call("setChoices", &[choices()]);

    let load = call("load", &[url(), 2.into(), false.into(), false.into()]);
    assert_eq!(load, "ok", "the page did not load");
    assert_eq!(
        (
            call("get", &[named("pageHeader"), named("title")]),
            call("get", &[named("pageHeader"), named("description")]),
        ),
        ("Notifications".to_string(), "Hikers".to_string()),
        "the page is not headed Notifications, with the chat's name"
    );
    assert_eq!(
        call("labels", &[]),
        "Notifications active|Mute for 1 hour(s)|Mute for 8 hour(s)|Mute for 1 day(s)|\
         Mute for 7 day(s)|Mute forever",
        "the page does not offer on, then the reference clients' lengths, in their order"
    );
    assert_eq!(
        call("shownOrder", &[]),
        "0,3600,28800,86400,604800,-1",
        "the page does not lay out on first, then the lengths shortest first"
    );
    assert_eq!(
        call("options", &[]),
        marked(Some(0)),
        "a chat not muted does not show notifications active"
    );
    assert_eq!(
        (
            call("get", &[named("saveHint"), named("text")]),
            call("get", &[named("mentionsNote"), named("visible")]),
        ),
        ("Swipe back to save.".to_string(), "false".to_string()),
        "the page does not say how it saves, or shows the mentions note uncalled for"
    );

    // Tap marks, sends nothing; another tap moves the mark.
    assert_eq!(call("click", &[named("muteOption28800")]), "ok");
    assert_eq!(call("click", &[named("muteOption-1")]), "ok");
    assert_eq!(
        (call("options", &[]), call("leave", &[])),
        (marked(Some(-1)), "-1".to_string()),
        "the last choice is not marked alone, or leaving did not save it"
    );
    // Second Deactivating (stub only): still once.
    assert_eq!(call("leave", &[]), "-1", "leaving twice saved twice");

    // Same choice as on entry: nothing to save.
    assert_eq!(
        call("load", &[url(), 2.into(), false.into(), false.into()]),
        "ok"
    );
    assert_eq!(call("click", &[named("muteOption3600")]), "ok");
    assert_eq!(call("click", &[named("muteOption0")]), "ok");
    assert_eq!(
        call("leave", &[]),
        "",
        "leaving on the choice the page opened with saved it again"
    );

    // Muted, end not known (another device): none marked, plain words; "on" unmutes.
    assert_eq!(
        call("load", &[url(), 2.into(), true.into(), true.into()]),
        "ok"
    );
    assert_eq!(
        (call("options", &[]), call("labels", &[])),
        (
            marked(None),
            "Notifications active|Mute for 1 hour(s)|Mute for 8 hour(s)|Mute for 1 day(s)|\
             Mute for 7 day(s)|Mute forever"
                .to_string()
        ),
        "a mute of unknown length marks a choice, or claims time left"
    );
    assert_eq!(
        call("get", &[named("mentionsNote"), named("visible")]),
        "true",
        "the mentions note is missing though replies still notify"
    );
    assert_eq!(call("click", &[named("muteOption0")]), "ok");
    assert_eq!(
        call("leave", &[]),
        "0",
        "notifications active on a muted chat did not unmute"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_mute_set_here_shows_its_length_and_time_left() {
    let temp = std::env::temp_dir().join(format!("piirit-mute-kept-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt and before server inherits.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }
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

    let core_ptr = std::ptr::addr_of!(core_box);
    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let steps: common::Steps = common::Steps::default();
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
    let row_url = || QString::from(common::component_url("ChatNotifications.qml"));
    let named = QString::from;

    let mut plan = common::Plan::default();
    // SAFETY for every block: fire only inside `exec()`; `engine`, `core_box`, `steps` outlive
    // it.
    plan.until(1, 30, move |_| unsafe { common::ready(&*core_ptr) });
    // Set before core ran: read now.
    plan.after(0, move || unsafe {
        call!("reloadChat");
    });
    let at = steps.clone();
    plan.until(1, 20, move |last| unsafe {
        if call!("chatState") != "true,false" && !last {
            return false;
        }
        call!("setChoices", choices());
        common::record(&at, "mute", call!("mute", 28800).into());
        true
    });
    let at = steps.clone();
    plan.until(1, 20, move |last| unsafe {
        if call!("chatState") != "true,true" && !last {
            return false;
        }
        common::record(&at, "page", call!("load", url(), 2, true, false).into());
        common::record(&at, "wide", call!("loadRow", row_url(), 2000).into());
        true
    });
    let at = steps.clone();
    plan.until(1, 20, move |last| unsafe {
        let read = call!("labels").contains("remaining") && call!("rowValue").contains("remaining");
        if !read && !last {
            return false;
        }
        common::record(&at, "options", call!("options").into());
        common::record(&at, "labels", call!("labels").into());
        common::record(&at, "wide-value", call!("rowValue").into());
        common::record(&at, "narrow", call!("loadRow", row_url(), 300).into());
        true
    });
    let at = steps.clone();
    plan.until(1, 20, move |last| unsafe {
        let shown = call!("rowValue");
        let short = shown.contains("remaining") && !shown.contains("for");
        if !short && !last {
            return false;
        }
        common::record(&at, "narrow-value", call!("rowValue").into());
        // Forever: no end to count down.
        common::record(&at, "forever", call!("mute", -1).into());
        true
    });
    let at = steps.clone();
    plan.until(1, 20, move |last| unsafe {
        if call!("rowValue") != "Muted forever" && !last {
            return false;
        }
        common::record(&at, "forever-value", call!("rowValue").into());
        common::record(
            &at,
            "forever-page",
            call!("load", url(), 2, true, false).into(),
        );
        true
    });
    let at = steps.clone();
    plan.until(1, 20, move |last| unsafe {
        let options = call!("options");
        if options != marked(Some(-1)) && !last {
            return false;
        }
        common::record(&at, "forever-options", options.into());
        common::record(&at, "unmute", call!("mute", 0).into());
        true
    });
    let at = steps.clone();
    plan.until(1, 20, move |last| unsafe {
        if call!("rowValue") != "On" && !last {
            return false;
        }
        common::record(&at, "unmuted-value", call!("rowValue").into());
        // Chat 1 muted per caller, nothing kept for it: length not known.
        common::record(
            &at,
            "unknown-page",
            call!("load", url(), 1, true, false).into(),
        );
        true
    });
    let at = steps.clone();
    plan.until(1, 20, move |last| unsafe {
        if call!("get", named("keptMute"), named("loaded")) != "true" && !last {
            return false;
        }
        common::record(&at, "unknown-options", call!("options").into());
        common::record(&at, "unknown-labels", call!("labels").into());
        (*engine_ptr).quit();
        true
    });
    plan.run();
    engine.exec();

    let steps = steps.borrow().clone();
    let methods: Vec<String> = common::methods(&journal);
    let context = format!("steps: {steps:?}\ncalls: {methods:?}");
    let value = |label: &str| common::value_of(&steps, label).to_string();
    for label in [
        "mute",
        "page",
        "wide",
        "narrow",
        "forever",
        "forever-page",
        "unmute",
        "unknown-page",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    // Clock read in the same second as the mute: whole length left.
    assert_eq!(
        (value("options"), value("labels")),
        (
            marked(Some(28800)),
            "Notifications active|Mute for 1 hour(s)|Mute for 8 hour(s) (8 h remaining)|\
             Mute for 1 day(s)|Mute for 7 day(s)|Mute forever"
                .to_string()
        ),
        "the page does not mark the kept length with its time left. {context}"
    );
    assert_eq!(
        (value("wide-value"), value("narrow-value")),
        (
            "Muted for 8 hour(s) (8 h remaining)".to_string(),
            "Muted (8 h remaining)".to_string()
        ),
        "the row does not show length and time left, or time left alone when narrow. \
         {context}"
    );
    assert_eq!(
        (value("forever-value"), value("forever-options")),
        ("Muted forever".to_string(), marked(Some(-1))),
        "a mute forever is not shown as such. {context}"
    );
    assert_eq!(
        value("unmuted-value"),
        "On",
        "the row does not say On once unmuted. {context}"
    );
    assert_eq!(
        value("unknown-options"),
        marked(None),
        "a mute this app did not keep marks a choice. {context}"
    );
    assert!(
        !value("unknown-labels").contains("remaining"),
        "a mute this app did not keep claims time left. {context}"
    );
    let _ = std::fs::remove_dir_all(&temp);
}
