//! Chat list is where a running app sits, so where core's own failures must show.
//!
//! Core going away lives in `qml_core_gone.rs`: needs a server that keeps dying, no environment to
//! read an error banner in.

// Qt harness: `unsafe` env::set_var pre-Qt (`unused_unsafe`: unsafe only from edition 2024),
// `borrow_as_ptr` for engine pointer, `single_shot` with whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Probe with components dir filled in: page's `Settings` singleton imported by absolute URL, as
/// probe loaded from data has no dir to resolve a relative one against.
fn probe_qml() -> String {
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import 'file://__COMPONENTS__'
    Item {
        Loader { id: loader }
        function load(url, accountId) {
            // Clear first, so loading the same page again really is a new
            // instance rather than the one already there.
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // Signals are callable, so the page can be driven without waiting
        // for the core to produce a real failure.
        function raise(message) { core.core_error(message) }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            // `data` rather than `children`: the chat model is a child of
            // the page but not a visual one, and `children` holds only
            // what is drawn.
            var kids = node.data !== undefined ? node.data : node.children
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
        // A search the profile cannot answer, so the list really is
        // empty and the placeholder is the only thing left to draw.
        function searchForNothing() {
            var model = findIn(loader.item, 'chats')
            if (!model) { return 'missing:chats' }
            model.query = 'zzqq-no-chat-is-called-this'
            return 'ok'
        }
        // What the next launch reads to know where to open.
        function remembered() { return '' + Settings.lastAccountId }
        function forget() { Settings.lastAccountId = 0; return 'ok' }
    }
";

#[test]
fn the_chat_list_shows_what_the_core_reports() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-chat-list-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt and before server inherits.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml().as_str()));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push(("forget", call!("forget")));
        (*steps_ptr).push(("before", call!("remembered")));
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1
            ),
        ));
        // Profile next launch opens on, written here: every way to a profile ends on this page.
        (*steps_ptr).push(("after", call!("remembered")));
        // Nothing read yet. Empty model means "no answer" this early, "no chats" later: opposites.
        (*steps_ptr).push((
            "placeholder-fresh",
            call!(
                "get",
                QString::from("chatListPlaceholder"),
                QString::from("enabled")
            ),
        ));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        call!("raise", QString::from("disk full"));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("search", call!("searchForNothing")));
        (*steps_ptr).push((
            "error-shown",
            call!("get", QString::from("errorLabel"), QString::from("text")),
        ));
        (*steps_ptr).push((
            "error-timeout",
            call!(
                "get",
                QString::from("errorBanner"),
                QString::from("timeout")
            ),
        ));
    });

    // Core answered, nothing to show: placeholder only thing.
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push((
            "placeholder-empty",
            call!(
                "get",
                QString::from("chatListPlaceholder"),
                QString::from("enabled")
            ),
        ));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// Failure on page: reader can wait it out.
fn assert_outcome(steps: &[(&str, String)]) {
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
        value("error-shown"),
        "disk full",
        "a core error reached no one. {context}"
    );
    assert_eq!(
        value("error-timeout"),
        "8",
        "an error the user can dismiss should clear itself. {context}"
    );

    assert_eq!(
        value("before"),
        "0",
        "the remembered profile did not start out cleared, so what \
         follows proves nothing. {context}"
    );
    assert_eq!(
        value("after"),
        "1",
        "the page did not remember the profile it is on, so the next \
         launch has nothing to open on and waits for the core. {context}"
    );

    assert_eq!(
        value("placeholder-fresh"),
        "false",
        "\"No chats yet\" is up before the core has said whether there \
         are any, so a phone that resumes onto this page is told it has \
         no chats and then handed a list of them. {context}"
    );
    assert_eq!(
        value("search"),
        "ok",
        "the chat model could not be reached, so the other half of this \
         proves nothing. {context}"
    );
    assert_eq!(
        value("placeholder-empty"),
        "true",
        "a profile with nothing in it says nothing at all, which reads \
         as a list that has not loaded. {context}"
    );
}
