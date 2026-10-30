//! Location's pieces on screen: tray entry, row pin, share dialog, sharing bar, feed. Map:
//! `qml_map.rs`.
//!
//! Stubs: no layout, no position. Whether the phone finds itself is a device question.

// Qt harness: see chat_actions.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::cell::RefCell;
use std::rc::Rc;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        // Unsized: a sized loader sizes item, hiding its own height.
        Loader { id: loader }
        property string trail: ''
        function load(url, properties) {
            loader.setSource('', {})
            trail = ''
            var given = JSON.parse(properties)
            given.width = 540
            loader.setSource(url, given)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // Each emission of `signal` noted with its arguments.
        function watch(signal) {
            loader.item[signal].connect(function () {
                var args = []
                for (var i = 0; i < arguments.length; i++) { args.push(arguments[i]) }
                trail += signal + '(' + args.join(',') + ');'
            })
            return 'ok'
        }
        function heard() { return trail }
        function set(property, value) { loader.item[property] = value; return 'ok' }
        function own(property) { return '' + loader.item[property] }
        function run(name) { return '' + loader.item[name]() }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function findAll(node, name, out) {
            if (!node) { return out }
            if (node.objectName === name) { out.push(node) }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) { findAll(kids[i], name, out) }
            return out
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function put(name, property, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item[property] = value
            return 'ok'
        }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        // Live choices, their times, and which is on.
        function live() {
            var all = findAll(loader.item, 'liveSwitch', [])
            var out = []
            for (var i = 0; i < all.length; i++) {
                out.push(all[i].text + '=' + all[i].seconds + (all[i].checked ? '*' : ''))
            }
            return out.join('|')
        }
        function pickLive(seconds) {
            var all = findAll(loader.item, 'liveSwitch', [])
            for (var i = 0; i < all.length; i++) {
                if (all[i].seconds === seconds) { all[i].clicked(); return 'ok' }
            }
            return 'missing:' + seconds
        }
        function fix(latitude, longitude, accuracy) {
            findIn(loader.item, 'position').report(latitude, longitude, accuracy)
            return 'ok'
        }
        // Last known fix, `age` ms old: what Sailfish hands first.
        function oldFix(latitude, longitude, accuracy, age) {
            findIn(loader.item, 'position').report(latitude, longitude, accuracy, age)
            return 'ok'
        }
        function lose(code) { findIn(loader.item, 'position').fail(code); return 'ok' }
        // Feed's position, as phone would hand it over.
        function feedFix(latitude, longitude, accuracy) {
            findIn(loader.item, 'livePosition').report(latitude, longitude, accuracy)
            return 'ok'
        }
        function feedOld(latitude, longitude, accuracy, age) {
            findIn(loader.item, 'livePosition').report(latitude, longitude, accuracy, age)
            return 'ok'
        }
        function feedLost() { findIn(loader.item, 'livePosition').forget(); return 'ok' }
        // Starts live sharing behind feed's back, as conversation page would.
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        function share(seconds) { chat.share_location(seconds); return 'ok' }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_place_is_offered_shown_and_fed_where_the_reader_looks() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-location-{}", std::process::id()));
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
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
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
                (*engine_ptr).invoke_method(
                    $name.into(),
                    &[$(QVariant::from(QString::from($arg))),*],
                )
            };
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }
    macro_rules! fix {
        ($name:expr, $latitude:expr, $longitude:expr, $accuracy:expr $(, $age:expr)?) => {{
            unsafe {
                (*engine_ptr).invoke_method(
                    $name.into(),
                    &[
                        QVariant::from($latitude),
                        QVariant::from($longitude),
                        QVariant::from($accuracy),
                        $(QVariant::from($age),)?
                    ],
                );
            }
        }};
    }

    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    // Tray: location entry asks, tray goes away. Location streaming off (experimental, as in
    // Delta Chat): no entry. Entries show once tray has faded in.
    plan.after(1, move || {
        call!("load", common::component_url("AttachButton.qml"), "{}");
        call!("set", "open", "true");
    });
    let s = steps.clone();
    plan.until(1, 20, move |last| {
        let done = call!("get", "attachTray", "visible") == QString::from("true");
        if done || last {
            common::record(&s, "tray-off", call!("get", "attachLocation", "visible"));
            common::record(
                &s,
                "tray",
                call!(
                    "load",
                    common::component_url("AttachButton.qml"),
                    r#"{"locationAvailable": true}"#
                ),
            );
            call!("watch", "locationRequested");
            call!("set", "open", "true");
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 20, move |last| {
        let done = call!("get", "attachTray", "visible") == QString::from("true");
        if done || last {
            common::record(&s, "tray-on", call!("get", "attachLocation", "visible"));
            call!("tap", "attachLocation");
            common::record(&s, "tray-asked", call!("heard"));
            common::record(&s, "tray-open", call!("own", "open"));
        }
        done
    });

    // Row with a point: pin and word, tap asks for map. Plain row: neither. Notice of live
    // sharing: lit, tap asks too. Notice never draws pin row.
    let s = steps.clone();
    plan.after(1, move || {
        let delegate = common::component_url("MessageDelegate.qml");
        // Location streaming off: no pin, tap asks nothing.
        call!(
            "load",
            delegate.clone(),
            r#"{"messageText": "https://www.openstreetmap.org/?mlat=1&mlon=2", "hasLocation": true}"#
        );
        call!("watch", "locationRequested");
        common::record(&s, "off-pin", call!("get", "locationRow", "visible"));
        common::record(&s, "off-link", call!("get", "messageLabel", "visible"));
        call!("run", "tapped");
        common::record(&s, "off-tap", call!("heard"));
        call!(
            "load",
            delegate.clone(),
            r#"{"messageText": "https://www.openstreetmap.org/?mlat=1&mlon=2", "hasLocation": true,
                "locationsEnabled": true}"#
        );
        call!("watch", "locationRequested");
        common::record(&s, "pin-shown", call!("get", "locationRow", "visible"));
        common::record(&s, "pin-word", call!("get", "locationLabel", "text"));
        common::record(&s, "pin-link", call!("get", "messageLabel", "visible"));
        call!("run", "tapped");
        common::record(&s, "pin-tap", call!("heard"));

        // Words beside link, or other link (sent while sharing live): body stays.
        call!(
            "load",
            delegate.clone(),
            r#"{"messageText": "Here: https://www.openstreetmap.org/?mlat=1&mlon=2",
                "hasLocation": true, "locationsEnabled": true}"#
        );
        common::record(&s, "worded-link", call!("get", "messageLabel", "visible"));
        call!(
            "load",
            delegate.clone(),
            r#"{"messageText": "https://example.org/", "hasLocation": true,
                "locationsEnabled": true}"#
        );
        common::record(&s, "other-link", call!("get", "messageLabel", "visible"));

        call!("load", delegate.clone(), r#"{"messageText": "hello"}"#);
        call!("watch", "locationRequested");
        common::record(&s, "plain-pin", call!("get", "locationRow", "visible"));
        common::record(&s, "plain-height", call!("get", "locationRow", "height"));
        call!("run", "tapped");
        common::record(&s, "plain-tap", call!("heard"));
        common::record(&s, "plain-info", call!("get", "infoLabel", "color"));

        call!(
            "load",
            delegate,
            r#"{"messageText": "Location streaming enabled by Ada.", "isInfo": true,
                "startsLocation": true, "hasLocation": true, "locationsEnabled": true}"#
        );
        call!("watch", "locationRequested");
        common::record(&s, "notice-pin", call!("get", "locationRow", "visible"));
        common::record(&s, "notice-pin-height", call!("get", "locationRow", "height"));
        common::record(&s, "notice-info", call!("get", "infoLabel", "color"));
        call!("run", "tapped");
        common::record(&s, "notice-tap", call!("heard"));
    });

    // Sharing bar: nothing unless sharing; stop asks.
    let s = steps.clone();
    plan.after(1, move || {
        let bar = common::component_url("SharingBar.qml");
        call!("load", bar.clone(), "{}");
        common::record(&s, "bar-idle", call!("own", "visible"));
        common::record(&s, "bar-idle-height", call!("own", "height"));
        call!("load", bar.clone(), r#"{"sharing": true}"#);
        call!("watch", "stopRequested");
        common::record(&s, "bar-on", call!("own", "visible"));
        common::record(&s, "bar-words", call!("get", "sharingLabel", "text"));
        common::record(&s, "bar-no-end", call!("get", "sharingLeft", "visible"));
        call!("tap", "stopSharingButton");
        common::record(&s, "bar-stop", call!("heard"));
        // Time left, against a set clock: 1000 s now.
        let mut left = Vec::new();
        for until in [1030, 2000, 4599, 4600, 5500, 6400, 87_400, 900] {
            call!(
                "load",
                bar.clone(),
                format!(r#"{{"sharing": true, "until": {until}, "now": 1000000}}"#)
            );
            left.push(call!("get", "sharingLeft", "text").to_string());
        }
        common::record(&s, "bar-left", left.join("|").into());
        call!("set", "stalled", "true");
        common::record(&s, "bar-stalled", call!("get", "sharingLabel", "text"));
    });

    // Dialog: once waits for a fix; live times; stop only while sharing.
    let s = steps.clone();
    plan.after(1, move || {
        let dialog = common::page_url("LocationShareDialog.qml");
        common::record(&s, "dialog", call!("load", dialog, "{}"));
        call!("watch", "locationPicked");
        call!("watch", "sharingPicked");
        common::record(&s, "dialog-times", call!("live"));
        common::record(&s, "dialog-stop", call!("get", "stopSwitch", "visible"));
        common::record(&s, "dialog-idle", call!("own", "canAccept"));
        common::record(&s, "dialog-finding", call!("get", "locationStatus", "text"));
        // No fix: live would send nothing, so refused like once.
        call!("pickLive", "1800");
        common::record(&s, "dialog-live-nofix", call!("own", "canAccept"));
        call!("tap", "nowSwitch");
        common::record(&s, "dialog-nofix", call!("own", "canAccept"));
        // Two minutes old: last known, not current.
        fix!(
            "oldFix",
            60.169_524_f64,
            24.935_456_f64,
            7.6_f64,
            120_000_f64
        );
        common::record(&s, "dialog-old", call!("own", "canAccept"));
        fix!("fix", 60.169_524_f64, 24.935_456_f64, 7.6_f64);
        common::record(&s, "dialog-fix", call!("own", "canAccept"));
        common::record(
            &s,
            "dialog-fix-status",
            call!("get", "locationStatus", "visible"),
        );
        call!("pickLive", "1800");
        common::record(&s, "dialog-live-fix", call!("own", "canAccept"));
        call!("tap", "nowSwitch");
        common::record(
            &s,
            "dialog-accuracy",
            call!("get", "nowSwitch", "description"),
        );
        fix!("fix", 60.169_524_f64, 24.935_456_f64, 0.0_f64);
        common::record(&s, "dialog-vague", call!("get", "nowSwitch", "description"));
        call!("run", "accept");
        common::record(&s, "dialog-once", call!("heard"));
    });
    let s = steps.clone();
    plan.after(1, move || {
        let dialog = common::page_url("LocationShareDialog.qml");
        call!("load", dialog.clone(), r#"{"sharing": true}"#);
        call!("watch", "locationPicked");
        call!("watch", "sharingPicked");
        common::record(&s, "sharing-stop", call!("get", "stopSwitch", "visible"));
        call!("tap", "stopSwitch");
        common::record(&s, "sharing-stop-nofix", call!("own", "canAccept"));
        call!("tap", "stopSwitch");
        fix!("fix", 60.2_f64, 25.0_f64, 5.0_f64);
        call!("pickLive", "1800");
        call!("pickLive", "86400");
        common::record(&s, "sharing-times", call!("live"));
        call!("run", "accept");
        call!("tap", "stopSwitch");
        common::record(&s, "sharing-now", call!("get", "nowSwitch", "checked"));
        call!("run", "accept");
        common::record(&s, "sharing-picked", call!("heard"));
        call!("load", dialog.clone(), "{}");
        call!("lose", "2");
        common::record(&s, "dialog-off", call!("get", "locationStatus", "text"));
        // Fix in hand, then source fails: refused.
        call!("load", dialog.clone(), "{}");
        fix!("fix", 60.2_f64, 25.0_f64, 5.0_f64);
        call!("tap", "nowSwitch");
        call!("lose", "2");
        common::record(&s, "dialog-off-accept", call!("own", "canAccept"));
        // Location off while dialog up: fixes stop, nothing says so but time.
        call!("load", dialog, r#"{"staleAfter": 200}"#);
        fix!("fix", 60.2_f64, 25.0_f64, 5.0_f64);
        call!("tap", "nowSwitch");
        common::record(&s, "dialog-before-lost", call!("own", "canAccept"));
    });
    let s = steps.clone();
    plan.until(1, 40, move |last| {
        let done = call!("get", "locationStatus", "visible") == QString::from("true");
        if done || last {
            common::record(&s, "dialog-lost", call!("get", "locationStatus", "text"));
            common::record(&s, "dialog-lost-accept", call!("own", "canAccept"));
            fix!("fix", 60.2_f64, 25.0_f64, 5.0_f64);
            common::record(&s, "dialog-found", call!("own", "canAccept"));
        }
        done
    });

    // Feed: still until a chat shares; then position on, phone awake, positions to core.
    let s = steps.clone();
    plan.after(1, move || {
        call!(
            "load",
            common::component_url("LocationFeed.qml"),
            r#"{"staleAfter": 300}"#
        );
    });
    let s2 = s.clone();
    plan.after(1, move || {
        common::record(&s2, "feed-idle", call!("get", "livePosition", "active"));
        common::record(
            &s2,
            "feed-idle-awake",
            call!("get", "locationKeepAlive", "enabled"),
        );
        call!("share", "1800");
    });
    plan.until(1, 10, move |last| {
        let done = call!("get", "livePosition", "active").to_string() == "true";
        if done || last {
            common::record(&s, "feed-on", call!("get", "livePosition", "active"));
            common::record(
                &s,
                "feed-awake",
                call!("get", "locationKeepAlive", "enabled"),
            );
            // Last known fix first, as Sailfish: not current, not sent.
            fix!("feedOld", 60.4_f64, 25.2_f64, 5.0_f64, 120_000_f64);
            fix!("feedFix", 60.2_f64, 25.0_f64, 8.0_f64);
            // Accuracy unknown: 0 to core. Fix lost: nothing.
            fix!("feedFix", 60.3_f64, 25.1_f64, 0.0_f64);
            common::record(&s, "feed-flowing", call!("own", "stalled"));
            call!("feedLost");
        }
        done
    });
    // No fresh fix for `staleAfter`: bar's warning.
    let s = steps.clone();
    plan.until(1, 40, move |last| {
        let done = call!("own", "stalled") == QString::from("true");
        if done || last {
            common::record(&s, "feed-stalled", call!("own", "stalled"));
            call!("share", "0");
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("get", "livePosition", "active").to_string() == "false";
        if done || last {
            common::record(&s, "feed-off", call!("get", "livePosition", "active"));
            common::record(
                &s,
                "feed-asleep",
                call!("get", "locationKeepAlive", "enabled"),
            );
            common::record(&s, "feed-off-stalled", call!("own", "stalled"));
            unsafe { (*engine_ptr).quit() };
        }
        done
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");
    let expect = |label: &str, wanted: &str, why: &str| {
        assert_eq!(value(label), wanted, "{label}: {why} {context}");
    };

    expect("tray-off", "false", "tray offers location while it is off.");
    expect("tray", "ok", "tray did not load.");
    expect("tray-on", "true", "tray hides location while it is on.");
    expect(
        "tray-asked",
        "locationRequested();",
        "tray's location entry asked nothing.",
    );
    expect(
        "tray-open",
        "false",
        "tray stayed open over the dialog it opened.",
    );

    expect("off-pin", "false", "a pin shows while location is off.");
    expect(
        "off-tap",
        "",
        "a point asked for a map while location is off.",
    );
    expect(
        "off-link",
        "true",
        "location off hides the only way to the place.",
    );
    expect("pin-shown", "true", "a row with a point shows no pin.");
    expect("pin-word", "Location", "");
    expect(
        "pin-link",
        "false",
        "the pin's own osm.org link still shows.",
    );
    expect("worded-link", "true", "words with the link hidden.");
    expect("other-link", "true", "a link not the pin's hidden.");
    expect(
        "pin-tap",
        "locationRequested();",
        "tapping a row's point asked for no map.",
    );
    expect("plain-pin", "false", "a plain row shows a pin.");
    expect("plain-height", "0", "a plain row keeps room for a pin.");
    expect("plain-tap", "", "tapping a plain row asked for a map.");
    expect("notice-pin", "false", "a core notice drew a pin row.");
    expect(
        "notice-pin-height",
        "0",
        "a core notice keeps room for a pin row.",
    );
    expect(
        "notice-tap",
        "locationRequested();",
        "live sharing's notice opens no map.",
    );
    assert_ne!(
        value("notice-info"),
        value("plain-info"),
        "live sharing's notice is not lit as tappable. {context}"
    );

    expect("bar-idle", "false", "bar shows while nothing is shared.");
    expect(
        "bar-idle-height",
        "0",
        "bar keeps room while nothing is shared.",
    );
    expect("bar-on", "true", "bar hides while sharing.");
    expect("bar-words", "You are sharing your location", "");
    expect("bar-no-end", "false", "time left shown with no end known.");
    expect(
        "bar-left",
        "1 minute(s) left|17 minute(s) left|60 minute(s) left|1 hour(s) left\
         |1 hour(s) left|2 hour(s) left|24 hour(s) left|1 minute(s) left",
        "time left not minutes rounded up under an hour, hours rounded after.",
    );
    expect(
        "bar-stalled",
        "Your location is not being sent. Is location turned on in Settings?",
        "bar says sharing while nothing goes out.",
    );
    expect("bar-stop", "stopRequested();", "bar's stop asked nothing.");

    expect("dialog", "ok", "dialog did not load.");
    expect(
        "dialog-times",
        "For 30 minutes=1800|For 1 hour=3600|For 2 hours=7200|For 6 hours=21600\
         |For 24 hours=86400",
        "live times are not the reference clients'.",
    );
    expect(
        "dialog-stop",
        "false",
        "stop offered while nothing is shared.",
    );
    expect(
        "dialog-idle",
        "false",
        "dialog accepts with nothing picked.",
    );
    expect("dialog-finding", "Finding your location…", "");
    expect(
        "dialog-live-nofix",
        "false",
        "live accepted before the phone knew where it is.",
    );
    expect("dialog-old", "false", "last known fix taken for current.");
    expect(
        "dialog-fix-status",
        "false",
        "status still shown with a fix in hand.",
    );
    expect(
        "dialog-live-fix",
        "true",
        "live refused with a fix in hand.",
    );
    expect(
        "dialog-nofix",
        "false",
        "once accepted before the phone knew where it is.",
    );
    expect("dialog-fix", "true", "once refused with a fix in hand.");
    expect(
        "dialog-accuracy",
        "Your location is accurate to 8 metre(s).",
        "accuracy not said in whole metres.",
    );
    expect(
        "dialog-vague",
        "",
        "accuracy said though the phone gave none.",
    );
    expect(
        "dialog-once",
        "locationPicked(60.169524,24.935456);",
        "accepting once did not hand over the fix.",
    );
    expect("sharing-stop", "true", "stop not offered while sharing.");
    expect(
        "sharing-stop-nofix",
        "true",
        "stop refused for want of a fix.",
    );
    expect(
        "sharing-times",
        "For 30 minutes=1800|For 1 hour=3600|For 2 hours=7200|For 6 hours=21600\
         |For 24 hours=86400*",
        "more than one time on, or the last pick not.",
    );
    expect("sharing-now", "false", "two choices on at once.");
    expect(
        "sharing-picked",
        "sharingPicked(86400);sharingPicked(0);",
        "live time or stop not handed over.",
    );
    expect(
        "dialog-off",
        "Turn on location in Settings to send it.",
        "a phone that cannot find itself says nothing.",
    );
    expect(
        "dialog-off-accept",
        "false",
        "accepted though the source failed.",
    );
    expect("dialog-before-lost", "true", "");
    expect(
        "dialog-lost",
        "Still looking for your location. Is location turned on in Settings?",
        "location gone while dialog up, dialog says nothing.",
    );
    expect(
        "dialog-lost-accept",
        "false",
        "accepted with the fix gone stale.",
    );
    expect("dialog-found", "true", "a fresh fix again still refused.");

    expect(
        "feed-idle",
        "false",
        "position looked for while nothing is shared.",
    );
    expect(
        "feed-idle-awake",
        "false",
        "phone kept awake while nothing is shared.",
    );
    expect("feed-on", "true", "position not looked for while sharing.");
    expect(
        "feed-awake",
        "true",
        "phone allowed to sleep while sharing.",
    );
    expect("feed-off", "false", "position still looked for after stop.");
    expect("feed-flowing", "false", "warned while fixes go out.");
    expect("feed-stalled", "true", "no warning while no fix goes out.");
    expect("feed-off-stalled", "false", "warning stays after stop.");
    expect("feed-asleep", "false", "phone kept awake after stop.");

    // Both fixes in one tick: sends race, order not kept (phone's come 30 s apart).
    let mut positions: Vec<String> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "set_location")
        .map(|(_, params)| params.to_string())
        .collect();
    positions.sort();
    assert_eq!(
        positions,
        ["[60.2,25.0,8.0]", "[60.3,25.1,0.0]"],
        "the feed's positions did not reach core, or a lost fix did. {context}"
    );
}
