//! Location: point sent once, chat getting it live, a row's place read back.
//!
//! Real core's shapes pinned in `deltachat-jsonrpc/tests/real_server.rs`. Here: what shim asks of
//! core, what QML sees.

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
        SharedLocation { id: place; account_id: 1; chat_id: 1 }
        LiveLocation { id: live }
        Connections {
            target: core
            onCore_event: {
                chat.handle_event(context_id, kind, payload_json)
                elsewhere.handle_event(context_id, kind, payload_json)
                place.handle_event(context_id, kind, payload_json)
                live.handle_event(context_id, kind, payload_json)
            }
        }
        property string failed: ''
        property int sentId: 0
        property int placeAnswers: 0
        Connections { target: chat; onError: failed = message; onSent: sentId = message_id }
        Connections { target: place; onPlace_changed: placeAnswers += 1 }
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
        // Message first: sender still 0 asks nothing, so one answer per look.
        function look(contact, id) {
            place.message_id = id
            place.contact_id = contact
            return 'ok'
        }
        function answers() { return '' + placeAnswers }
        function shown() {
            return [place.loaded, place.found, place.live, place.latitude, place.longitude,
                    place.accuracy, place.timestamp].join(',')
        }
        function map() { return place.map_url }
        function link() { return place.link }
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

    // Ada's one-off point, then her live sharing's notice.
    let s = steps.clone();
    plan.after(0, move || {
        common::record(&s, "rows", {
            let one_off = call!("row", 30).to_string();
            let notice = call!("row", 31).to_string();
            let other = call!("row", 2).to_string();
            QString::from(format!("{one_off}|{notice}|{other}"))
        });
        call!("look", 10, 30);
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("answers").to_string() != "0";
        if done || last {
            common::record(&s, "own-point", call!("shown"));
            common::record(&s, "own-map", call!("map"));
            common::record(&s, "own-link", call!("link"));
            call!("look", 10, 31);
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("answers").to_string() == "2";
        if done || last {
            common::record(&s, "newest", call!("shown"));
        }
        done
    });

    // Reader's own point, once; then one the world has no place for.
    plan.after(0, move || {
        call!("send", 60.169_524_f64, 24.935_456_f64);
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let sent = call!("sent").to_string();
        let done = sent != "0";
        if done || last {
            let id: i32 = sent.parse().unwrap_or_default();
            common::record(&s, "sent-row", call!("row", id));
            call!("send", 91.0_f64, 0.0_f64);
            common::record(&s, "nowhere", call!("error"));
            // Reader's own place from here: their positions move it.
            call!("look", 1, id);
        }
        done
    });

    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("answers").to_string() == "3";
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
            call!("openLater");
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("laterSharing").to_string() == "true";
        if done || last {
            common::record(&s, "opened-later", call!("laterSharing"));
            call!("report", 60.2_f64, 25.0_f64, 8.0_f64);
        }
        done
    });
    // Position stored: core says so; sender's newest asked again.
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("answers").to_string() == "4";
        if done || last {
            common::record(&s, "moved", call!("answers"));
            call!("share", 0);
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("sharing").to_string() == "false,false,false";
        if done || last {
            common::record(&s, "stopped", call!("sharing"));
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
    assert_eq!(
        value("own-point"),
        "true,true,false,60.1,24.9,0,1700090000",
        "a one-off point did not show itself, fixed. {context}"
    );
    assert_eq!(
        value("own-map"),
        "https://www.openstreetmap.org/export/embed.html\
         ?bbox=24.89398%2C60.09700%2C24.90602%2C60.10300\
         &layer=mapnik&marker=60.10000%2C24.90000",
        "{context}"
    );
    assert_eq!(
        value("own-link"),
        "https://www.openstreetmap.org/?mlat=60.10000&mlon=24.90000#map=16/60.10000/24.90000",
        "{context}"
    );
    assert_eq!(
        value("newest"),
        "true,true,true,60.2,25,12,1700090600",
        "live sharing's notice did not show its sender's newest place. {context}"
    );
    assert_eq!(
        value("sent-row"),
        "true,false,1",
        "the point sent is not on its row. {context}"
    );
    assert_eq!(value("nowhere"), "no location to send", "{context}");
    assert!(
        value("sent-point").starts_with("true,true,false,60.169524,24.935456,"),
        "the point sent does not show itself. {context}"
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
        "4",
        "the map did not ask again when its sender moved. {context}"
    );
    assert_eq!(
        value("stopped"),
        "false,false,false",
        "chat or feed did not hear live sharing stop. {context}"
    );
    assert_eq!(value("short"), "true,true,true", "{context}");
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
        [&json!([1, 1, 1800]), &json!([1, 1, 0]), &json!([1, 1, 30])],
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
    let asked: Vec<&Value> = calls
        .iter()
        .filter(|(method, _)| method == "get_locations")
        .map(|(_, params)| params)
        .collect();
    let (ada, own) = (json!([1, 1, 10, 0, 0]), json!([1, 1, 1, 0, 0]));
    assert!(
        asked
            .iter()
            .all(|params| **params == ada || **params == own)
            && asked.last() == Some(&&own),
        "a row's place asked of the wrong chat or sender, or not again on moving: {calls:?}"
    );
}
