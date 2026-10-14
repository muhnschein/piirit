//! Relative time keeps following the clock, not frozen at draw time.
//!
//! Issue 108: chat list said "now" for days-old chat. `Format.timeLabel` read `Date.now()` in a
//! binding, which QML re-evaluates only when row's own timestamp changes; pinning or reading
//! changed other roles, so only a restart fixed it. Now present is `Clock.now`, a singleton
//! ticking while app is on screen. Test drives it by hand and checks every relative-time row.

// Qt harness: `unsafe` `env::set_var` before Qt starts (`unused_unsafe`: unsafe only from
// edition 2024), `borrow_as_ptr` for engine pointer, `single_shot` with whole-second Durations.
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

/// Probe with components dir filled in, so it reaches `Clock` singleton as a page does.
fn probe_qml() -> String {
    // Canonical: probe and rows name one dir, so share one singleton.
    let components = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../qml/components")
        .canonicalize()
        .expect("the components directory");
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import 'file://__COMPONENTS__'
    Item {
        Loader { id: chatRow }
        Loader { id: searchRow }
        function load() {
            chatRow.setSource(Qt.resolvedUrl('file://__COMPONENTS__/ChatListDelegate.qml'),
                              { width: 540, chatName: 'Ada', preview: 'a video' })
            searchRow.setSource(Qt.resolvedUrl('file://__COMPONENTS__/SearchResultRow.qml'),
                                { width: 540, title: 'Ada' })
            return chatRow.status === Loader.Ready && searchRow.status === Loader.Ready
                   ? 'ok' : 'load-failed'
        }
        // Both rows told about a message from a few seconds ago, in the
        // unit each takes: Unix seconds.
        function arrived() {
            var seconds = Math.floor(Date.now() / 1000) - 5
            chatRow.item.lastUpdated = seconds
            searchRow.item.timestamp = seconds
            return 'ok'
        }
        // Two hours on, without a word to either row.
        function later() {
            Clock.now = Date.now() + 2 * 3600 * 1000
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
        function chatTime() {
            var label = findIn(chatRow.item, 'timeLabel')
            return label ? '' + label.text : 'missing:timeLabel'
        }
        function searchTime() {
            var label = findIn(searchRow.item, 'resultTime')
            return label ? '' + label.text : 'missing:resultTime'
        }

        // The clock itself. A headless run has no application state to
        // change, so the test says whether the app is on screen.
        function shown(active) { Clock.appActive = active; return 'ok' }
        function quick() { Clock.interval = 50; return 'ok' }
        function stale() { Clock.now = 1; return 'ok' }
        // How far the clock is behind the real time, in whole seconds.
        function behind() { return '' + Math.round((Date.now() - Clock.now) / 1000) }
    }
";

#[test]
fn a_time_label_follows_the_clock_rather_than_the_moment_it_was_drawn() {
    // SAFETY: single-threaded test; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("TZ", "UTC");
        std::env::set_var("LC_ALL", "C");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(probe_qml().as_str()));

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
        record!("load", call!("load"));
        // Off screen: only the test moves the clock.
        call!("shown", false);
        call!("arrived");
        record!("chat-fresh", call!("chatTime"));
        record!("search-fresh", call!("searchTime"));
        call!("later");
        record!("chat-later", call!("chatTime"));
        record!("search-later", call!("searchTime"));

        // Now clock alone: fast tick, left stale, on screen.
        call!("quick");
        call!("stale");
        call!("shown", true);
        // On screen: reads time now, not next tick.
        record!("on-screen", call!("behind"));
        call!("shown", false);
        call!("stale");
    });

    // Off screen 1s: would be 20 ticks at 50ms.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("off-screen", call!("behind"));
        call!("shown", true);
        call!("stale");
        // Stale after on-screen: only timer moves it.
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("ticking", call!("behind"));
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

    assert_eq!(value("load"), "ok", "the rows did not load. {context}");
    assert_eq!(
        value("chat-fresh"),
        "now",
        "a chat row does not say a message from moments ago is new. {context}"
    );
    assert_eq!(
        value("search-fresh"),
        "now",
        "a search result does not say a message from moments ago is new. {context}"
    );
    // The bug: two hours on, row still said "now".
    assert_eq!(
        value("chat-later"),
        "2 h",
        "a chat row kept the age it had when it was drawn. {context}"
    );
    assert_eq!(
        value("search-later"),
        "2 h",
        "a search result kept the age it had when it was drawn. {context}"
    );
    assert_eq!(
        value("on-screen"),
        "0",
        "coming back on screen left the clock where it was. {context}"
    );
    // Stale by whole epoch: nothing read time off screen.
    assert!(
        value("off-screen").parse::<i64>().unwrap_or_default() > 1_000_000,
        "the clock ticked with nothing on screen to need it. {context}"
    );
    assert_eq!(
        value("ticking"),
        "0",
        "the clock did not tick while on screen. {context}"
    );
}
