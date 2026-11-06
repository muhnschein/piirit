//! Location: point sent once, chat getting it live, map opened on a row's place.
//!
//! Real core's shapes pinned in `deltachat-jsonrpc/tests/real_server.rs`. Here: what shim asks of
//! core, what QML sees. Map app's serving: `webxdc_map.rs`.

// Qt harness: see chat_actions.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::cell::RefCell;
use std::rc::Rc;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::{json, Value};

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        // Same chat open elsewhere: hears of sharing only through core's event.
        ChatMessages { id: elsewhere; account_id: 1; chat_id: 1 }
        // Opened while sharing, deaf to events: must ask on load.
        ChatMessages { id: later; account_id: 1 }
        LocationMap { id: map; account_id: 1 }
        LiveLocation { id: live }
        Connections {
            target: core
            onCore_event: {
                chat.handle_event(context_id, kind, payload_json)
                elsewhere.handle_event(context_id, kind, payload_json)
                live.handle_event(context_id, kind, payload_json)
                // Map app told of each stored position, as core does.
                if (kind === 'WebxdcStatusUpdate') {
                    moves += 1
                }
            }
        }
        property string failed: ''
        property int sentId: 0
        property int moves: 0
        property var opened: []
        Connections { target: chat; onError: failed = message; onSent: sentId = message_id }
        Connections {
            target: map
            onOpened: opened = opened.concat([message_id + query])
            onError: opened = opened.concat(['error:' + message])
        }
        Repeater {
            id: rows
            model: chat.rows
            Item {
                property int messageId: model.message_id
                property bool located: model.has_location
                property bool starts: model.starts_location
                property int sender: model.sender_id
            }
        }
        function loaded() { return '' + chat.loaded }
        function row(id) {
            for (var i = 0; i < rows.count; i++) {
                var item = rows.itemAt(i)
                if (item && item.messageId === id) {
                    return item.located + ',' + item.starts + ',' + item.sender
                }
            }
            return 'missing'
        }
        function look(chatId, contact, id) { map.open(chatId, contact, id); return 'ok' }
        function answers() { return '' + opened.length }
        function shown() { return opened[opened.length - 1] || '' }
        function moved() { return '' + moves }
        function send(latitude, longitude) {
            chat.send_location(latitude, longitude)
            return 'ok'
        }
        function sent() { return '' + sentId }
        function error() { return failed }
        function share(seconds) { chat.share_location(seconds); return 'ok' }
        function sharing() {
            return chat.sharing_location + ',' + live.active + ',' + elsewhere.sharing_location
        }
        function openLater() { later.chat_id = 1; return 'ok' }
        function laterSharing() { return '' + later.sharing_location }
        // When sharing ends, as each chat has it.
        function ends() {
            return chat.sharing_until + ',' + elsewhere.sharing_until + ',' + later.sharing_until
        }
        function refresh() { live.refresh(); return 'ok' }
        function report(latitude, longitude, accuracy) {
            live.report(latitude, longitude, accuracy)
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_place_goes_once_or_live_and_a_row_shows_where_its_sender_is() {
    let temp = std::env::temp_dir().join(format!("piirit-location-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        std::env::set_var("PIIRIT_FAKE_LOCATIONS", "1");
        // Map app written here, not runner's cache.
        std::env::set_var("XDG_CACHE_HOME", temp.join("cache"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let steps: common::Steps = Rc::new(RefCell::new(Vec::new()));

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = unsafe {
                (*engine_ptr).invoke_method($name.into(), &[$(QVariant::from($arg)),*])
            };
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }

    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    plan.until(1, 10, move |_| call!("loaded").to_string() == "true");

    // Ada's one-off point, then her live sharing's notice, then every chat.
    let s = steps.clone();
    plan.after(0, move || {
        common::record(&s, "rows", {
            let one_off = call!("row", 30).to_string();
            let notice = call!("row", 31).to_string();
            let other = call!("row", 2).to_string();
            QString::from(format!("{one_off}|{notice}|{other}"))
        });
        call!("look", 1, 10, 30);
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("answers").to_string() == "1";
        if done || last {
            common::record(&s, "own-point", call!("shown"));
            call!("look", 1, 10, 31);
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("answers").to_string() == "2";
        if done || last {
            common::record(&s, "notice", call!("shown"));
            call!("look", 0, 0, 0);
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("answers").to_string() == "3";
        if done || last {
            common::record(&s, "every", call!("shown"));
            call!("send", 60.169_524_f64, 24.935_456_f64);
        }
        done
    });

    // Reader's own point, once; then one the world has no place for.
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let sent = call!("sent").to_string();
        let done = sent != "0";
        if done || last {
            let id: i32 = sent.parse().unwrap_or_default();
            common::record(&s, "sent-row", call!("row", id));
            call!("send", 91.0_f64, 0.0_f64);
            common::record(&s, "nowhere", call!("error"));
            call!("look", 1, 1, id);
        }
        done
    });

    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("answers").to_string() == "4";
        if done || last {
            common::record(&s, "sent-point", call!("shown"));
            // Live for half an hour; core's `ChatModified` tells chat and feed.
            call!("refresh");
        }
        done
    });
    let s = steps.clone();
    plan.after(1, move || {
        common::record(&s, "before", call!("sharing"));
        call!("share", 1800);
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("sharing").to_string() == "true,true,true";
        if done || last {
            common::record(&s, "sharing", call!("sharing"));
            common::record(&s, "sharing-ends", call!("ends"));
            call!("openLater");
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("laterSharing").to_string() == "true";
        if done || last {
            common::record(&s, "opened-later", call!("laterSharing"));
            common::record(&s, "later-ends", call!("ends"));
            call!("report", 60.2_f64, 25.0_f64, 8.0_f64);
        }
        done
    });
    // Position stored: open map hears of it.
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("moved").to_string() != "0";
        if done || last {
            common::record(&s, "moved", call!("moved"));
            // Longer while sharing: end moves, sharing as was.
            call!("share", 7200);
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        // Past half an hour from now: the two hours' end.
        let soon = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs())
            + 3600;
        let ends = call!("ends").to_string();
        let done = ends
            .split(',')
            .take(2)
            .all(|end| end.parse::<u64>().unwrap_or_default() > soon);
        if done || last {
            common::record(&s, "longer-ends", call!("ends"));
            call!("share", 0);
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("sharing").to_string() == "false,false,false";
        if done || last {
            common::record(&s, "stopped", call!("sharing"));
            common::record(&s, "stopped-ends", call!("ends"));
            // Nobody wants it: not handed to core.
            call!("report", 1.0_f64, 2.0_f64, 0.0_f64);
            // Runs out before next position, core's word on it lost.
            call!("share", 30);
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("sharing").to_string() == "true,true,true";
        if done || last {
            common::record(&s, "short", call!("sharing"));
            call!("report", 3.0_f64, 4.0_f64, 0.0_f64);
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("sharing").to_string().contains(",false,");
        if done || last {
            common::record(&s, "ran-out", call!("sharing"));
            unsafe { (*engine_ptr).quit() };
        }
        done
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("rows"),
        "true,false,10|false,true,10|false,false,10",
        "rows do not say which carries a place and which starts live sharing. {context}"
    );
    // One app, opened four ways. Every chat's: framing all (Ada's two points, fake core).
    let every = value("every");
    let app = every
        .strip_suffix("?bounds=60.1,24.9,60.2,25")
        .unwrap_or_default()
        .to_string();
    assert!(
        !app.is_empty() && app.bytes().all(|b| b.is_ascii_digit()),
        "every chat's map did not open framing every location. {context}"
    );
    assert_eq!(
        value("own-point"),
        format!("{app}?poi=60.1,24.9"),
        "a one-off point's map did not open on it. {context}"
    );
    assert_eq!(
        value("notice"),
        format!("{app}?contactId=10"),
        "live sharing's notice did not open the map on its sender. {context}"
    );
    assert_eq!(
        value("sent-row"),
        "true,false,1",
        "the point sent is not on its row. {context}"
    );
    assert_eq!(value("nowhere"), "no location to send", "{context}");
    assert_eq!(
        value("sent-point"),
        format!("{app}?poi=60.169524,24.935456"),
        "the point sent does not open the map on itself. {context}"
    );
    assert_eq!(value("before"), "false,false,false", "{context}");
    assert_eq!(
        value("sharing"),
        "true,true,true",
        "chat or feed did not hear live sharing start. {context}"
    );
    assert_eq!(
        value("opened-later"),
        "true",
        "a chat opened while sharing does not say so. {context}"
    );
    assert_eq!(
        value("moved"),
        "1",
        "core did not take the position, or the map was not told. {context}"
    );
    assert_eq!(
        value("stopped"),
        "false,false,false",
        "chat or feed did not hear live sharing stop. {context}"
    );
    assert_eq!(value("short"), "true,true,true", "{context}");
    // Half an hour from when asked; a minute's slack for a slow runner.
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let ends: Vec<u64> = value("later-ends")
        .split(',')
        .map(|end| end.parse().unwrap_or_default())
        .collect();
    assert!(
        ends.len() == 3
            && ends.iter().all(|end| *end == ends[0])
            && ends[0].abs_diff(started + 1800) < 120,
        "chat, chat open elsewhere, or chat opened later does not know when sharing ends. \
         {context}"
    );
    assert_eq!(
        value("sharing-ends").split(',').take(2).collect::<Vec<_>>(),
        [ends[0].to_string(), ends[0].to_string()],
        "end not known as soon as sharing starts. {context}"
    );
    let longer: Vec<u64> = value("longer-ends")
        .split(',')
        .map(|end| end.parse().unwrap_or_default())
        .collect();
    assert!(
        longer.len() == 3 && longer[0] == longer[1] && longer[0].abs_diff(started + 7200) < 120,
        "a longer share while sharing did not move the end. {context}"
    );
    // Later's chat is deaf to events: only first two hear of stop.
    assert!(
        value("stopped-ends").starts_with("0,0,"),
        "an end kept after sharing stopped. {context}"
    );
    assert_eq!(
        value("ran-out"),
        "true,false,true",
        "the feed kept going after core said no chat wants a position. {context}"
    );

    let calls = common::calls(&journal);
    let sends: Vec<&Value> = calls
        .iter()
        .filter(|(method, _)| method == "misc_send_msg")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        sends,
        [&json!([
            1,
            1,
            "https://www.openstreetmap.org/?mlat=60.16952&mlon=24.93546#map=16/60.16952/24.93546",
            null,
            null,
            [60.169_524, 24.935_456],
            null
        ])],
        "one point, carried and linked, should have gone: {calls:?}"
    );
    let shares: Vec<&Value> = calls
        .iter()
        .filter(|(method, _)| method == "send_locations_to_chat")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        shares,
        [
            &json!([1, 1, 1800]),
            &json!([1, 1, 7200]),
            &json!([1, 1, 0]),
            &json!([1, 1, 30])
        ],
        "{calls:?}"
    );
    let positions: Vec<&Value> = calls
        .iter()
        .filter(|(method, _)| method == "set_location")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        positions,
        [&json!([60.2, 25.0, 8.0]), &json!([3.0, 4.0, 0.0])],
        "positions went to core while nobody wanted them, or not at all: {calls:?}"
    );
    let of = |name: &str| -> Vec<&Value> {
        calls
            .iter()
            .filter(|(method, _)| method == name)
            .map(|(_, params)| params)
            .collect()
    };
    // No app in core yet: bundled one handed over once, version noted; later opens reuse it.
    let installed = of("set_webxdc_integration");
    assert_eq!(
        installed.len(),
        1,
        "map app not handed to core exactly once: {calls:?}"
    );
    let path = installed[0][1].as_str().unwrap_or_default();
    assert!(
        path.ends_with("piirit/piirit/maps/1.xdc") && !std::path::Path::new(path).exists(),
        "map app not written to cache, or left there after core copied it: {calls:?}"
    );
    assert!(
        of("set_config")
            .iter()
            .any(|params| params[1] == "ui.piirit.maps_version"
                && params[2].as_str().is_some_and(|value| value.len() == 40)),
        "core not told which map app it has: {calls:?}"
    );
    assert_eq!(
        of("init_webxdc_integration"),
        [
            &json!([1, 1]),
            &json!([1, 1]),
            &json!([1, null]),
            &json!([1, 1])
        ],
        "map app not pointed at chat, or at every chat: {calls:?}"
    );
    let asked = of("get_locations");
    let (whole, rows): (Vec<&Value>, Vec<&Value>) =
        asked.into_iter().partition(|params| params[2].is_null());
    assert_eq!(
        rows,
        [
            &json!([1, 1, 10, 0, 0]),
            &json!([1, 1, 10, 0, 0]),
            &json!([1, 1, 1, 0, 0])
        ],
        "a row's point asked of the wrong chat or sender, or with no row: {calls:?}"
    );
    // Every chat's map: every chat, everyone, the day the map app shows.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    assert!(
        whole.len() == 1
            && whole[0][1].is_null()
            && whole[0][4] == 0
            && whole[0][3]
                .as_u64()
                .is_some_and(|from: u64| from.abs_diff(now - 86_400) < 120),
        "every chat's map not asked for the last day of every chat: {calls:?}"
    );
}
