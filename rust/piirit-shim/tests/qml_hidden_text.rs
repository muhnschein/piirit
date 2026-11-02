//! Parts a message row doesn't show: not made (quote, call, full-text offer), or hold no text.
//!
//! Hidden `Text` is still laid out: body wrapped twice per message (info label), sender name and
//! call lines, date on every row. Every part made costs too: plain row was ~40 items.

// Qt harness: see qml_conversation_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

/// Plain `ListModel` with real roles: three plain messages, same day, one-to-one chat.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 800

        ListModel { id: rows }
        Loader { id: loader; anchors.fill: parent }

        function load(url) {
            for (var i = 0; i < 3; i++) {
                rows.append({
                    message_id: rows.count + 1,
                    text: 'a plain message, number ' + (rows.count + 1),
                    is_outgoing: false, is_info: false, show_padlock: true,
                    state: 16, timestamp: 1700000000 + rows.count,
                    day_number: 19675, sender_name: 'Ada 🙂',
                    sender_color: '#00875a', quote_text: '', quote_author: '',
                    file_path: '', file_name: '', view_type: 'Text',
                    image_width: 0, image_height: 0, loaded: true
                })
            }
            loader.setSource(url, { model: rows })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function allIn(node, name, found) {
            if (!node) { return found }
            if (node.objectName === name) { found.push(node) }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                allIn(kids[i], name, found)
            }
            if (node.contentItem && node.contentItem !== node) {
                allIn(node.contentItem, name, found)
            }
            return found
        }
        function rowsInOrder() {
            var found = allIn(loader.item, 'messageRow', [])
            var once = []
            for (var i = 0; i < found.length; i++) {
                if (once.indexOf(found[i]) === -1) { once.push(found[i]) }
            }
            once.sort(function (one, other) { return one.y - other.y })
            return once
        }
        // Per row: each named part's text, '' when it holds none. 'missing' when not found.
        function texts(names) {
            var out = []
            var found = rowsInOrder()
            for (var i = 0; i < found.length; i++) {
                var parts = []
                for (var n = 0; n < names.length; n++) {
                    var part = allIn(found[i], names[n], [])
                    parts.push(names[n] + '=' + (part.length > 0 ? part[0].text : 'missing'))
                }
                out.push(parts.join(','))
            }
            return out.join(' / ')
        }
        // Sender name with emoji as pictures: worked out per row unless hidden.
        function pictured() {
            var out = []
            var found = rowsInOrder()
            for (var i = 0; i < found.length; i++) {
                var label = allIn(found[i], 'senderLabel', [])
                out.push(label.length > 0 ? label[0].pictured : 'missing')
            }
            return out.join('/')
        }
        function count() { return '' + rowsInOrder().length }
        function setRole(row, role, value) {
            rows.setProperty(row, role, value)
            return 'ok'
        }
        // Named part's height in middle row, against what it holds: 'part:inner'.
        function room(name, inner) {
            var row = rowsInOrder()[1]
            var part = allIn(row, name, [])
            var held = allIn(row, inner, [])
            if (part.length === 0 || held.length === 0) { return 'missing' }
            return Math.round(part[0].height) + ':' + Math.round(held[0].height)
        }
        function heights() {
            var out = []
            var found = rowsInOrder()
            for (var i = 0; i < found.length; i++) {
                out.push(Math.round(found[i].contentHeight))
            }
            return out.join(',')
        }
    }
";

/// Parts a plain message from the other person in a one-to-one chat never shows, kept made.
const HIDDEN: [&str; 5] = [
    "infoLabel",
    "senderLabel",
    "forwardedLabel",
    "fullMetric",
    "downloadButton",
];

/// Parts made only for rows that show them.
const NOT_MADE: [&str; 9] = [
    "quoteArea",
    "quoteAuthor",
    "quoteLabel",
    "callIcon",
    "callTitle",
    "callDetail",
    "callTitleMetric",
    "callDetailMetric",
    "fullButton",
];

#[test]
fn a_plain_message_row_makes_and_lays_out_only_what_it_shows() {
    // SAFETY: single-threaded; set before Qt starts.
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
            let result = (*engine_ptr).invoke_method($name.into(), &[$(QVariant::from($arg)),*]);
            QString::from_qvariant(result).map(|value| value.to_string()).unwrap_or_default()
        }};
    }
    let names = |list: &[&str]| {
        QVariant::from(
            list.iter()
                .map(|name| QVariant::from(QString::from(*name)))
                .collect::<QVariantList>(),
        )
    };

    let list = common::component_url("ConversationList.qml");
    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("load", call!("load", QString::from(list.clone()))));
    });
    plan.after(2, move || unsafe {
        (*steps_ptr).push(("rows", call!("count")));
        (*steps_ptr).push(("hidden", call!("texts", names(&HIDDEN))));
        (*steps_ptr).push(("not-made", call!("texts", names(&NOT_MADE))));
        (*steps_ptr).push(("pictured", call!("pictured")));
        (*steps_ptr).push(("day", call!("texts", names(&["dayLabel"]))));
        (*steps_ptr).push(("unread", call!("texts", names(&["unreadLabel"]))));
        (*steps_ptr).push(("body", call!("texts", names(&["messageLabel"]))));
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
    let context = format!("steps: {steps:?}");
    assert_eq!(value("load"), "ok", "{context}");
    assert_eq!(value("rows"), "3", "{context}");
    let row = HIDDEN
        .iter()
        .map(|name| format!("{name}="))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        value("hidden"),
        [row.as_str(); 3].join(" / "),
        "a row holds text in a part it does not show. {context}"
    );
    assert_eq!(
        value("pictured"),
        "//",
        "a hidden sender name was turned into pictures. {context}"
    );
    let row = NOT_MADE
        .iter()
        .map(|name| format!("{name}=missing"))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        value("not-made"),
        [row.as_str(); 3].join(" / "),
        "a row made a part it does not show. {context}"
    );
    // Date on the day's first row only; no "New messages" line anywhere.
    let day = value("day");
    let days: Vec<&str> = day.split(" / ").collect();
    assert!(
        days.len() == 3 && days[0].len() > "dayLabel=".len(),
        "the day's first row has no date. {context}"
    );
    assert_eq!(
        &days[1..],
        ["dayLabel=", "dayLabel="],
        "rows after the day's first hold its date. {context}"
    );
    assert_eq!(
        value("unread"),
        "unreadLabel= / unreadLabel= / unreadLabel=",
        "{context}"
    );
    // The body itself is drawn.
    assert!(
        value("body").contains("a plain message, number 3"),
        "{context}"
    );
}

/// Too long to draw whole: row offers the rest.
fn a_long_message() -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for number in 1..=40 {
        let _ = writeln!(out, "- [ ] the {number}th thing to do today");
    }
    out
}

/// Quote or offer gone from a row (edit shortened it): its room goes with it. Loader keeps last
/// item's height after unloading it.
#[test]
#[allow(clippy::too_many_lines)]
fn a_part_that_goes_gives_its_room_back() {
    // SAFETY: single-threaded; set before Qt starts.
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
            let result = (*engine_ptr).invoke_method($name.into(), &[$(QVariant::from($arg)),*]);
            QString::from_qvariant(result).map(|value| value.to_string()).unwrap_or_default()
        }};
    }
    macro_rules! set {
        ($role:expr, $value:expr) => {
            call!("setRole", 1, QString::from($role), QString::from($value))
        };
    }

    let list = common::component_url("ConversationList.qml");
    let long = a_long_message();
    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("load", call!("load", QString::from(list.clone()))));
    });
    plan.after(2, move || unsafe {
        (*steps_ptr).push(("base", call!("heights")));
        set!("quote_author", "Bo");
        set!("quote_text", "an earlier message");
    });
    plan.after(2, move || unsafe {
        (*steps_ptr).push(("quoted", call!("heights")));
        (*steps_ptr).push((
            "quote-room",
            call!(
                "room",
                QString::from("quoteRow"),
                QString::from("quoteLabel")
            ),
        ));
        set!("quote_text", "");
    });
    plan.after(2, move || unsafe {
        (*steps_ptr).push(("unquoted", call!("heights")));
        set!("text", long.as_str());
    });
    plan.after(2, move || unsafe {
        (*steps_ptr).push(("long", call!("heights")));
        let offer = QVariant::from(
            [QVariant::from(QString::from("fullButton"))]
                .into_iter()
                .collect::<QVariantList>(),
        );
        (*steps_ptr).push(("offer", call!("texts", offer)));
        (*steps_ptr).push((
            "offer-room",
            call!(
                "room",
                QString::from("bodyActions"),
                QString::from("fullButton")
            ),
        ));
        set!("text", "a plain message, number 2");
    });
    plan.after(2, move || unsafe {
        (*steps_ptr).push(("short", call!("heights")));
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
    let heights = |label: &str| -> Vec<i64> {
        value(label)
            .split(',')
            .filter_map(|part| part.parse().ok())
            .collect()
    };
    let context = format!("steps: {steps:?}");
    assert_eq!(value("load"), "ok", "{context}");
    let base = heights("base");
    assert_eq!(base.len(), 3, "{context}");
    assert!(
        heights("quoted")[1] > base[1],
        "the quote took no room. {context}"
    );
    // Part at least as tall as what it holds: else next part drawn over it.
    let fits = |label: &str| {
        let room = value(label);
        let parts: Vec<i64> = room
            .split(':')
            .filter_map(|part| part.parse().ok())
            .collect();
        parts.len() == 2 && parts[1] > 0 && parts[0] >= parts[1]
    };
    assert!(fits("quote-room"), "the quote has no room. {context}");
    assert_eq!(
        heights("unquoted"),
        base,
        "the quote's room stayed. {context}"
    );
    assert!(heights("long")[1] > base[1], "{context}");
    assert!(
        value("offer").contains("fullButton=View full message"),
        "a long message has no offer. {context}"
    );
    assert!(fits("offer-room"), "the offer has no room. {context}");
    assert_eq!(heights("short"), base, "the offer's room stayed. {context}");
}
