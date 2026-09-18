//! Picking an app from the store.
//!
//! The store is a website the `WebView` loads, so what is worth testing
//! is the part that is not the website: that a tapped `.xdc` link is
//! taken out of the engine's hands and fetched through the core, that
//! what lands is a file on the phone with the store's own name on it,
//! and that the page reports it the way a picker reports a chosen file.
//!
//! The view is a stub, so nothing here says the store draws. What it
//! fakes is how the tap arrives: the frame script the page loads into the
//! engine stops the click and sends the address back as a message, which
//! is the route that works on a phone -- a .xdc is downloaded rather than
//! navigated to, so watching the view's address never hears about it.
//! A navigation is faked too, to prove the same app is not taken twice.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`, recorded rather than performed: the page pops
/// itself once an app has been taken.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    pop: qt_method!(fn(&mut self)),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap) -> QVariant),
}

impl StackProbe {
    fn pop(&mut self) {
        let current = self.log.to_string();
        self.log = format!("{current}pop;").into();
        self.log_changed();
    }

    // The page pushes the file browser from here; nothing is loaded for
    // it, which the page survives by checking what it got back.
    fn push(&mut self, page: QString, _properties: QVariantMap) -> QVariant {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let current = self.log.to_string();
        self.log = format!("{current}push:{name};").into();
        self.log_changed();
        QVariant::default()
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        property string chosen: ''
        Loader { id: loader }
        function load(url, accountId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.picked.connect(function (path) { chosen = path })
            return 'ok'
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
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // The frame script's message, as the engine delivers it.
        function deliver(message, where) {
            var view = findIn(loader.item, 'storeView')
            if (!view) { return 'missing:storeView' }
            view.recvAsyncMessage(message, { uri: where })
            return 'ok'
        }
        // A navigation that commits, as the engine reports it.
        function follow(where) {
            var view = findIn(loader.item, 'storeView')
            if (!view) { return 'missing:storeView' }
            view.url = where
            return 'ok'
        }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function picked() { return chosen }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_tapped_app_is_fetched_by_the_core_and_reported_as_a_file() {
    let temp = std::env::temp_dir().join(format!("piirit-webxdc-store-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let cache = temp.join("cache");
    std::fs::create_dir_all(&cache).expect("create cache dir");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("XDG_CACHE_HOME", &cache);
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(StackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let stack_ptr = std::ptr::addr_of!(stack_box);
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

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("WebxdcStorePage.qml")),
                1
            )
        );
        record!(
            "opened",
            call!("get", QString::from("storeView"), QString::from("url"))
        );
        record!(
            "heading",
            call!("get", QString::from("storeHeader"), QString::from("title"))
        );
        record!(
            "from-phone",
            call!(
                "get",
                QString::from("fromPhoneButton"),
                QString::from("text")
            )
        );
        record!(
            "listeners",
            call!(
                "get",
                QString::from("storeView"),
                QString::from("listeners")
            )
        );
        record!(
            "frame-scripts",
            call!(
                "get",
                QString::from("storeView"),
                QString::from("frameScripts")
            )
        );
        // A link to an app, tapped: the frame script stopped it and said
        // where it went.
        record!(
            "tapped",
            call!(
                "deliver",
                QString::from("piirit:app"),
                QString::from("https://webxdc.org/apps/checkers.xdc?v=2")
            )
        );
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("picked", call!("picked"));
        record!("popped", (*stack_ptr).pinned().borrow().log.to_string());
        record!(
            "banner",
            call!("get", QString::from("errorBanner"), QString::from("text"))
        );
        // The same tap, arriving again the other way. One app was taken;
        // this is not a second one.
        record!(
            "follow",
            call!(
                "follow",
                QString::from("https://webxdc.org/apps/checkers.xdc?v=2")
            )
        );
        // The way out when the store cannot be reached: the file browser.
        record!("tap-phone", call!("tap", QString::from("fromPhoneButton")));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("pushed", (*stack_ptr).pinned().borrow().log.to_string());
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

    assert_eq!(
        value("load"),
        "ok",
        "the store page did not load. {context}"
    );
    assert_eq!(
        value("opened"),
        "https://webxdc.org/apps/",
        "the page did not open the store. {context}"
    );
    assert_eq!(
        value("heading"),
        "Apps",
        "the page has no heading of its own before the store says what it \
         is called. {context}"
    );
    assert_eq!(
        value("from-phone"),
        "From the phone",
        "the page offers no way to an app already on the phone, which is \
         also the way out when the store cannot be reached. {context}"
    );

    assert!(
        value("listeners").contains("piirit:app"),
        "the page did not listen for its frame script's message, so a \
         tapped app would never reach it: {}. {context}",
        value("listeners")
    );
    assert!(
        value("frame-scripts").contains("webxdc/catch.js"),
        "the page did not load the frame script that catches a tap on an \
         app: {}. {context}",
        value("frame-scripts")
    );
    assert_eq!(
        value("tapped"),
        "ok",
        "the tapped app was not delivered. {context}"
    );
    assert_eq!(
        value("follow"),
        "ok",
        "the link was not followed. {context}"
    );
    let picked = value("picked");
    assert!(
        picked.ends_with("checkers.xdc"),
        "the app was not fetched under the name the store gave it: {picked}. \
         {context}"
    );
    assert!(
        std::path::Path::new(&picked).exists(),
        "the app was reported at {picked}, where there is no file. {context}"
    );
    assert_eq!(
        std::fs::read(&picked).unwrap_or_default(),
        b"PK\x03\x04 a fake app",
        "what was saved is not what the core handed over. {context}"
    );
    assert!(
        value("popped").contains("pop;"),
        "the store stayed open after an app was taken from it. {context}"
    );
    assert_eq!(
        value("banner"),
        "",
        "the page reported an error: {}. {context}",
        value("banner")
    );
    assert!(
        value("pushed").contains("push:AttachAppPage.qml;"),
        "the button did not open the file browser. {context}"
    );

    // The download went through the core, which is the whole point: the
    // engine would have put it somewhere this app cannot reach.
    let calls = common::calls(&journal);
    let fetched: Vec<&(String, serde_json::Value)> = calls
        .iter()
        .filter(|(name, _)| name == "get_http_response")
        .collect();
    assert_eq!(
        fetched.len(),
        1,
        "the app should have been fetched exactly once, through the core, \
         however many ways the same tap arrived: {calls:?}"
    );
    assert_eq!(
        fetched[0]
            .1
            .pointer("/1")
            .and_then(serde_json::Value::as_str),
        Some("https://webxdc.org/apps/checkers.xdc?v=2"),
        "the core was asked for a different URL than the one followed"
    );
}
