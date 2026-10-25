//! Webxdc app page: what it points view at, and what it says until it can.
//!
//! View is a stub (engine is a device question). Checked: page starts app, hands view the
//! address shim served, names app in header, offers source only if app has one, stops serving
//! on way out. Leaving = page destroyed (pop or stack replace), not covered: `sendToChat` opens
//! picker over app at its request, so stopping on deactivate killed that request. Address
//! asked, not assumed: stopped host refuses connection.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`, recorded. Pops on message delete.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    pop: qt_method!(fn(&mut self)),
}

impl StackProbe {
    fn pop(&mut self) {
        let current = self.log.to_string();
        self.log = format!("{current}pop;").into();
        self.log_changed();
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url, accountId, messageId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                messageId: messageId,
                appName: 'Checkers'
            })
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
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // The core says the message is gone, which is the one thing that
        // takes the page away by itself. Built with JSON.stringify so the
        // payload is quoted the way the shim's own events are.
        function deleted() {
            var app = findIn(loader.item, 'app')
            if (!app) { return 'missing:app' }
            app.handle_event(1, 'WebxdcInstanceDeleted',
                             JSON.stringify({ msgId: 5 }))
            return 'ok'
        }
        // Something went wrong, as the shim's own signal reports it.
        function fail(message) {
            loader.item.errorMessage = message
            return 'ok'
        }
        // The view saying it has arrived, which is the engine's job and
        // the one thing a stub cannot do by itself.
        function arrived() {
            var view = findIn(loader.item, 'webxdcView')
            if (!view) { return 'missing:webxdcView' }
            view.loadProgress = 100
            view.loaded = true
            return 'ok'
        }
        // An app trying to take the view somewhere else, which is what
        // window.location does.
        function wander(where) {
            var view = findIn(loader.item, 'webxdcView')
            if (!view) { return 'missing:webxdcView' }
            view.url = where
            return '' + view.url
        }
        // Another page opening over this one, which is what the chat
        // picker `sendToChat` asks for does. The page is covered, not
        // gone.
        function cover() {
            loader.item.status = PageStatus.Deactivating
            return 'ok'
        }
        // And back on screen, the page over it gone.
        function uncover() {
            loader.item.status = PageStatus.Active
            return 'ok'
        }
        // Leaving for good: a popped page is destroyed, and so is one
        // whose stack was replaced.
        function leave() {
            loader.setSource('', {})
            return 'ok'
        }
    }
";

/// Value recorded under `label`.
fn value_of(steps: &[(&str, String)], label: &str) -> String {
    steps
        .iter()
        .find(|(name, _)| *name == label)
        .map(|(_, value)| value.clone())
        .unwrap_or_default()
}

/// Whether anything still listens where app was served: stopped host refuses connection.
/// Only way to tell running app from stopped; page holding it may be gone.
fn reachable(url: &str) -> String {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    let Some(authority) = rest.split('/').next().filter(|at| at.contains(':')) else {
        return format!("no-address:{url}");
    };
    match std::net::TcpStream::connect(authority) {
        Ok(_) => "yes".to_string(),
        Err(_) => "no".to_string(),
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_page_runs_the_app_the_shim_serves_and_stops_it_on_the_way_out() {
    let temp = std::env::temp_dir().join(format!("piirit-webxdc-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("XDG_CACHE_HOME", temp.join("cache"));
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

    let mut plan = common::Plan::default();

    plan.after(2, move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("WebxdcPage.qml")),
                1,
                5
            )
        );
        // Host not answered yet: page says working.
        record!(
            "early-url",
            call!("get", QString::from("webxdcView"), QString::from("url"))
        );
        record!(
            "early-busy",
            call!("get", QString::from("webxdcBusy"), QString::from("running"))
        );
        record!(
            "early-title",
            call!("get", QString::from("webxdcHeader"), QString::from("title"))
        );
    });

    plan.after(2, move || unsafe {
        record!(
            "url",
            call!("get", QString::from("webxdcView"), QString::from("url"))
        );
        // Served, not drawn: page says what it waits on.
        record!(
            "serving-busy",
            call!("get", QString::from("webxdcBusy"), QString::from("running"))
        );
        record!(
            "serving-said",
            call!("get", QString::from("webxdcWaiting"), QString::from("text"))
        );
        // Reason shown on page: banner clears itself in seconds; a never-drawn view would say
        // nothing.
        record!(
            "failed",
            call!("fail", QString::from("cannot serve the app: no such file"))
        );
        record!(
            "failed-said",
            call!("get", QString::from("webxdcWaiting"), QString::from("text"))
        );
        record!(
            "failed-shown",
            call!(
                "get",
                QString::from("webxdcWaiting"),
                QString::from("visible")
            )
        );
        record!(
            "failed-busy",
            call!("get", QString::from("webxdcBusy"), QString::from("running"))
        );
        record!("cleared", call!("fail", QString::from("")));
        // The engine says it has the app.
        record!("arrived", call!("arrived"));
        record!(
            "busy",
            call!("get", QString::from("webxdcBusy"), QString::from("running"))
        );
        record!(
            "said",
            call!(
                "get",
                QString::from("webxdcWaiting"),
                QString::from("visible")
            )
        );
        record!(
            "title",
            call!("get", QString::from("webxdcHeader"), QString::from("title"))
        );
        record!(
            "banner",
            call!("get", QString::from("errorBanner"), QString::from("text"))
        );
        // Off its own address, back home.
        record!(
            "wander",
            call!("wander", QString::from("https://example.org/pay"))
        );
        record!(
            "served",
            call!("get", QString::from("webxdcView"), QString::from("url"))
        );
        record!("cover", call!("cover"));
    });

    plan.after(1, move || unsafe {
        // Covered: app still there and served; app that asked for picker awaits its answer.
        record!(
            "after-cover",
            call!("get", QString::from("webxdcView"), QString::from("url"))
        );
        record!(
            "served-covered",
            reachable(&value_of(&*steps_ptr, "served"))
        );
        record!("deleted", call!("deleted"));
    });

    plan.after(1, move || unsafe {
        // Not popped while covered: a pop would take the page on top.
        record!(
            "popped-covered",
            (*stack_ptr).pinned().borrow().log.to_string()
        );
        call!("uncover");
        record!("popped", (*stack_ptr).pinned().borrow().log.to_string());
        record!("leave", call!("leave"));
    });

    plan.after(1, move || unsafe {
        // Left: page and its host gone.
        record!(
            "after-leave",
            call!("get", QString::from("webxdcView"), QString::from("url"))
        );
        record!("served-after", reachable(&value_of(&*steps_ptr, "served")));
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

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("early-url"),
        "",
        "the view was pointed somewhere before the app was served. {context}"
    );
    assert_eq!(
        value("early-busy"),
        "true",
        "the page said nothing while the app was coming up. {context}"
    );
    assert_eq!(
        value("early-title"),
        "Checkers",
        "the page did not carry over the name the row already knew. {context}"
    );

    assert!(
        value("url").starts_with("http://127.0.0.1:"),
        "the view was not pointed at the app the shim serves: {}. {context}",
        value("url")
    );
    assert!(
        value("url").ends_with("/index.html"),
        "the view was not pointed at the app's own page: {}. {context}",
        value("url")
    );
    assert_eq!(
        value("serving-busy"),
        "true",
        "the page stopped saying it was working while the app had still \
         drawn nothing. {context}"
    );
    assert_eq!(
        value("serving-said"),
        "Starting the app",
        "the page did not say it was working while the app had drawn \
         nothing. {context}"
    );
    assert_eq!(value("failed"), "ok", "the page took no reason. {context}");
    assert_eq!(
        value("failed-said"),
        "cannot serve the app: no such file",
        "an app that will not open left the reader nothing to read once \
         the banner had cleared itself. {context}"
    );
    assert_eq!(
        value("failed-shown"),
        "true",
        "the reason an app will not open is not on the screen. {context}"
    );
    assert_eq!(
        value("failed-busy"),
        "false",
        "the page is still saying it is working on an app that failed. \
         {context}"
    );
    assert_eq!(
        value("cleared"),
        "ok",
        "the reason did not clear. {context}"
    );
    assert_eq!(value("arrived"), "ok", "the view did not arrive. {context}");
    assert_eq!(
        value("busy"),
        "false",
        "the page is still saying it is working after the app came up. {context}"
    );
    assert_eq!(
        value("said"),
        "false",
        "the page is still saying what it is waiting for after the app \
         came up. {context}"
    );
    assert_eq!(
        value("title"),
        "Checkers",
        "the header does not name the app the core says it is. {context}"
    );
    assert_eq!(
        value("banner"),
        "",
        "the page reported an error: {}. {context}",
        value("banner")
    );

    assert!(
        value("wander").starts_with("http://127.0.0.1:"),
        "an app navigated itself off its own address and stayed there: {}. \
         {context}",
        value("wander")
    );

    assert!(
        value("served").starts_with("http://127.0.0.1:"),
        "the app was never served, so there is nothing to stop: {}. \
         {context}",
        value("served")
    );
    assert_eq!(
        value("after-cover"),
        value("served"),
        "a page opened over the app took the app away from under it. \
         `sendToChat` opens the chat picker over the app at the app's own \
         request, so this is the app losing the answer to the very \
         request that asked. {context}"
    );
    assert_eq!(
        value("served-covered"),
        "yes",
        "nothing is listening where the app was served while its page is \
         merely covered: an app waiting on the picker it asked for is \
         waiting on a host that has stopped. {context}"
    );
    assert_eq!(
        value("after-leave"),
        "missing:webxdcView",
        "the page outlived being left. {context}"
    );
    assert_eq!(
        value("served-after"),
        "no",
        "the app is still being served after the page was left. {context}"
    );
    assert_eq!(
        value("popped-covered"),
        "",
        "a page running an app that was deleted while another page was over \
         it popped that page instead of itself. {context}"
    );
    assert_eq!(
        value("popped"),
        "pop;",
        "a page running an app that has been deleted stayed open. {context}"
    );
}
