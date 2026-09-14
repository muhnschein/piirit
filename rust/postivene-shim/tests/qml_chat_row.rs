//! What a chat-list row shows: the unread badge, when the last message
//! landed, who wrote it, and how the chat is filed.

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
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 540 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        // Seconds, so the row is told a time rather than an offset.
        function ago(days) {
            return Math.floor(Date.now() / 1000) - days * 86400
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
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
    }
";

#[test]
fn a_chat_row_shows_its_unread_count_time_and_marks() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("TZ", "UTC");
        // Fixes what the weekday and date formats render as.
        std::env::set_var("LC_ALL", "C");
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
    macro_rules! set {
        ($property:expr, $value:expr) => {
            call!("set", QString::from($property), $value)
        };
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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("ChatListDelegate.qml"))
            )
        );
        set!("chatName", QString::from("Ada Lovelace"));
        set!("preview", QString::from("see you there"));
        set!("previewSender", QString::from("Ada"));
        set!("chatColor", QString::from("#00875a"));
        // A message from earlier today.
        let today = call!("ago", 0.0);
        set!("lastUpdated", today.parse::<f64>().unwrap_or_default());
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("no-badge", get!("unreadBadge", "visible"));
        record!("today", get!("timeLabel", "text"));
        record!("preview", get!("previewLabel", "text"));
        record!("name", get!("nameLabel", "text"));

        set!("unreadCount", 3.0);
        // Older than today but inside the week.
        let recent = call!("ago", 3.0);
        set!("lastUpdated", recent.parse::<f64>().unwrap_or_default());
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("badge", get!("unreadBadge", "visible"));
        record!("badge-text", get!("unreadLabel", "text"));
        record!("this-week", get!("timeLabel", "text"));

        set!("unreadCount", 120.0);
        set!("isPinned", true);
        set!("isMuted", true);
        set!("isEncrypted", false);
        // Our own last message, delivered.
        set!("summaryState", 26.0);
        set!("previewSender", QString::from("Ada"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("badge-capped", get!("unreadLabel", "text"));
        record!("marked-name", get!("nameLabel", "text"));
        record!("pinned-mark", get!("pinMark", "visible"));
        record!("muted-mark", get!("muteMark", "visible"));
        record!("ours", get!("previewLabel", "text"));

        // Older than a week: a date, not a weekday that would read as one
        // of the last seven days.
        let old = call!("ago", 30.0);
        set!("lastUpdated", old.parse::<f64>().unwrap_or_default());
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("older", get!("timeLabel", "text"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// Everything a row has to say at a glance.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the row did not load. {context}");
    assert_eq!(
        value("no-badge"),
        "false",
        "a chat with nothing unread wore a badge. {context}"
    );
    // Relative, not a clock time: the question a chat list answers is
    // "how recently", and "now" answers it without the reader having to
    // know what time it is.
    assert_eq!(
        value("today"),
        "now",
        "a message from moments ago is not shown as such. {context}"
    );
    assert_eq!(
        value("preview"),
        "Ada: see you there",
        "someone else's message does not say who sent it. {context}"
    );
    assert_eq!(
        value("name"),
        "Ada Lovelace",
        "the row does not name its chat. {context}"
    );
    assert_eq!(
        value("badge"),
        "true",
        "unread messages went unbadged. {context}"
    );
    assert_eq!(value("badge-text"), "3", "the badge miscounts. {context}");
    // The source form: this loads no catalog, and "%n day(s)" is what
    // the source says. The app always has one -- the reader's language,
    // or the English catalog for its plural forms -- and
    // postivene-app's translation test is what proves the word comes out.
    assert_eq!(
        value("this-week"),
        "3 day(s)",
        "a message from earlier this week is not counted in days. {context}"
    );
    assert!(
        value("older").len() > 4 && !value("older").contains(':'),
        "a message from a month ago is not shown as a date: {}. {context}",
        value("older")
    );
    assert_eq!(
        value("badge-capped"),
        "99+",
        "a large unread count is not capped. {context}"
    );
    // The name is a name. Pinned and muted say where the chat sits and
    // how it behaves, so they live on the right with the time; only the
    // mail icon, which is about the chat itself, stays on the name.
    assert_eq!(
        value("marked-name"),
        "✉ Ada Lovelace",
        "the name carries marks that belong on the right. {context}"
    );
    assert_eq!(
        value("pinned-mark"),
        "true",
        "a pinned chat does not show it. {context}"
    );
    assert_eq!(
        value("muted-mark"),
        "true",
        "a muted chat does not show it. {context}"
    );
    assert_eq!(
        value("ours"),
        "✓ see you there",
        "our own last message shows a sender instead of its delivery mark. {context}"
    );
}
