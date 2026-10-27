//! Root QML and all it reaches load, opening on right page.
//!
//! Regression: cover's `showChats` signal removed while root still handled it; `Cannot assign
//! to non-existent property "onShowChats"` took window down (white screen). Grep can't catch
//! it: handler capitalises signal (`onShowChats`). Loading the file does.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Probe with window's components dir filled in: data-loaded probe has no dir for relative
/// imports, and this reads a dconf key via window's `Settings` singleton.
fn probe_qml(components: &std::path::Path) -> String {
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import 'file://__COMPONENTS__'
    Item {
        Loader { id: loader }
        function load(url) {
            // Cleared first: a Loader handed the URL it already has
            // keeps the window it already built, and what is under test
            // is the choice a new one makes.
            loader.setSource('', {})
            loader.setSource(url, {})
            if (loader.status === Loader.Error) {
                return 'error: ' + loader.sourceComponent
            }
            return loader.status === Loader.Ready ? 'ok' : 'not-ready'
        }
        /// What this phone remembers about the profile it was last on.
        function remember(id) {
            Settings.lastAccountId = parseInt(id, 10)
            return 'ok'
        }
        /// Which page the window chose to open on, and for a chat list
        /// the profile it is on. A `Component` takes no property but its
        /// id, so the two are told apart by what they turn out to be:
        /// only the chat list has a profile, only the first screen has a
        /// probe.
        function chosenPage() {
            if (!loader.item || !loader.item.initialPage) { return 'no-page' }
            var made = loader.item.initialPage.createObject(loader.item, {})
            if (!made) { return 'page-failed' }
            if (made.accountId !== undefined) { return 'chats:' + made.accountId }
            if (made.probing !== undefined) { return 'first-screen' }
            return 'unknown'
        }
        /// The cover is a Component on the window; instantiating it is
        /// what would have found the handler for a signal that no longer
        /// exists, since a Component is not built until it is used.
        function buildCover() {
            if (!loader.item || !loader.item.cover) { return 'no-cover' }
            var made = loader.item.cover.createObject(loader.item, {})
            return made ? 'ok' : 'cover-failed'
        }
        function buildInitialPage() {
            if (!loader.item || !loader.item.initialPage) { return 'no-page' }
            var made = loader.item.initialPage.createObject(loader.item, {})
            return made ? 'ok' : 'page-failed'
        }
    }
";

#[test]
fn the_application_window_and_what_it_holds_all_load() {
    let temp = std::env::temp_dir().join(format!("piirit-startup-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // No `EnterKey` stub; `ConversationPage` not reached anyway: copy keeps one reason to fail.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded; before Qt.
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
    // Real one handed in by main.rs; never read here.
    engine.set_property(
        "rpcServerPath".into(),
        QString::from(env!("CARGO_BIN_EXE_fake-core-server")).into(),
    );
    engine.load_data(QByteArray::from(
        probe_qml(&tree.join("components")).as_str(),
    ));

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

    let root = format!("file://{}", tree.join("piirit.qml").display());

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        // Used phone. dconf outlives test binary: both cases set, not assumed.
        (*steps_ptr).push(("remember", call!("remember", QString::from("7"))));
        (*steps_ptr).push(("resumed-load", call!("load", QString::from(root.clone()))));
        (*steps_ptr).push(("resumed-choice", call!("chosenPage")));
        // Unused phone.
        (*steps_ptr).push(("forget", call!("remember", QString::from("0"))));
        (*steps_ptr).push(("load", call!("load", QString::from(root.clone()))));
        (*steps_ptr).push(("fresh-choice", call!("chosenPage")));
        (*steps_ptr).push(("cover", call!("buildCover")));
        (*steps_ptr).push(("initial-page", call!("buildInitialPage")));
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

    assert_eq!(
        value("load"),
        "ok",
        "the root QML does not load, so the app is a white screen. {context}"
    );
    assert_eq!(
        value("cover"),
        "ok",
        "the cover does not build. A Component is not checked until it is \
         instantiated, so a handler for a signal that no longer exists sits \
         there silently until the app is launched. {context}"
    );
    assert_eq!(
        value("initial-page"),
        "ok",
        "the first page the app shows does not build. {context}"
    );

    // What window opens on: chat list, vs first screen drawn and taken away again.
    assert_eq!(
        value("resumed-load"),
        "ok",
        "the root QML does not load on a phone that remembers a profile. \
         {context}"
    );
    assert_eq!(
        value("resumed-choice"),
        "chats:7",
        "a phone that remembers a profile does not open on that \
         profile's chat list. Going through the welcome page means the \
         first screen -- field, name, buttons -- is drawn and then taken \
         away again, for as long as the hand-over takes. {context}"
    );
    assert_eq!(
        value("fresh-choice"),
        "first-screen",
        "a phone with no profile does not open on the first screen, \
         which is the only thing it has to show. {context}"
    );
}
