//! What a chat is sits to the right of the conversation, attached.
//!
//! The group page was reachable only by tapping the header, and nothing
//! said the header could be tapped. An attached page is announced by the
//! page indicator and reached by a swipe, which is the platform's way of
//! saying "there is more this way". Which page is attached depends on the
//! kind of chat, so the conversation waits for the load; and the header
//! tap now goes the same way the swipe does.
//!
//! The page is loaded from a copy of the QML tree with `EnterKey` taken
//! out; see `common::qml_tree_without_enter_key`.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`, recorded rather than performed: what was
/// attached, and whether the forward navigation was taken.
// Named as QML calls them: qmetaobject exposes a method under its Rust
// name, and Silica's are camel-cased.
#[allow(non_snake_case)]
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `attach:GroupPage.qml|forward|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    /// The page named by the last `pushAttached`, empty until one.
    attached: qt_property!(QString; NOTIFY log_changed),

    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    pushAttached: qt_method!(fn(&mut self, page: QString, properties: QVariantMap) -> QVariant),
    nextPage: qt_method!(fn(&self, page: QVariant) -> QVariant),
    navigateForward: qt_method!(fn(&mut self)),
}

#[allow(non_snake_case)]
impl PageStackProbe {
    fn note(&mut self, entry: &str) {
        let current = self.log.to_string();
        self.log = format!("{current}{entry}|").into();
        self.log_changed();
    }

    fn push(&mut self, page: QString, _properties: QVariantMap) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        self.note(&format!("push:{name}"));
    }

    fn pushAttached(&mut self, page: QString, _properties: QVariantMap) -> QVariant {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        self.attached = name.clone().into();
        self.note(&format!("attach:{name}"));
        // No page to hand back: the conversation only connects to a
        // group's `renamed`, and a null here says there is nothing to
        // connect to, as Silica does for a page that failed to load.
        QVariant::default()
    }

    /// Whether something is attached. Silica answers with the page; the
    /// conversation only asks whether there is one.
    fn nextPage(&self, _page: QVariant) -> QVariant {
        if self.attached.to_string().is_empty() {
            QVariant::default()
        } else {
            QVariant::from(true)
        }
    }

    fn navigateForward(&mut self) {
        self.note("forward");
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        // Created the way pageStack.push does, still on its way in, and
        // then settled: attaching waits for both the load and the page.
        function load(url, chatId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: 1,
                chatId: chatId,
                chatName: 'chat ' + chatId,
                status: PageStatus.Activating
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function settle() { loader.item.status = PageStatus.Active; return 'ok' }
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
        function loaded() {
            var model = findIn(loader.item, 'messages')
            return model ? '' + model.loaded : 'missing:messages'
        }
        function tapHeader() {
            var tap = findIn(loader.item, 'headerTap')
            if (!tap) { return 'missing:headerTap' }
            tap.clicked(null)
            return 'ok'
        }
    }
";

#[test]
fn the_chat_info_page_is_attached_to_the_right() {
    let temp = std::env::temp_dir().join(format!("piirit-attached-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(PageStackProbe::default());
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
    // Read from inside the steps the way the engine is: the box outlives
    // `exec()`, and the closures fire only while it runs.
    let stack_ptr: *const QObjectBox<PageStackProbe> = std::ptr::addr_of!(stack_box);
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

    // A group, chat 2. Loaded while still arriving, so nothing can be
    // attached yet however fast the core answers.
    let group_url = common::page_url_in(&tree, "ConversationPage.qml");
    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "load-group",
            call!("load", QString::from(group_url.clone()), 2)
        );
    });
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("group-loaded", call!("loaded"));
        record!(
            "before-active",
            (*stack_ptr).pinned().borrow().log.to_string()
        );
        record!("settle-group", call!("settle"));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        record!(
            "group-attached",
            (*stack_ptr).pinned().borrow().attached.to_string()
        );
        // Settling again must not attach a second one.
        record!("resettle", call!("settle"));
        record!("tap", call!("tapHeader"));
        record!("group-log", (*stack_ptr).pinned().borrow().log.to_string());
        // Now a one-to-one chat, chat 1, on a fresh stack.
        (*stack_ptr).pinned().borrow_mut().attached = QString::default();
    });
    let single_url = common::page_url_in(&tree, "ConversationPage.qml");
    single_shot(Duration::from_secs(6), move || unsafe {
        record!(
            "load-single",
            call!("load", QString::from(single_url.clone()), 1)
        );
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("settle-single", call!("settle"));
    });
    single_shot(Duration::from_secs(9), move || unsafe {
        record!(
            "single-attached",
            (*stack_ptr).pinned().borrow().attached.to_string()
        );
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_attached(&steps);
}

/// The right page for the kind of chat, attached once the page is there,
/// only once, and reached by the header tap as well.
fn assert_attached(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    for label in [
        "load-group",
        "settle-group",
        "resettle",
        "tap",
        "load-single",
        "settle-single",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert_eq!(
        value("group-loaded"),
        "true",
        "the group chat never loaded. {context}"
    );
    assert_eq!(
        value("before-active"),
        "",
        "a page was attached while the conversation was still arriving. {context}"
    );
    assert_eq!(
        value("group-attached"),
        "GroupPage.qml",
        "the group's page is not attached to the conversation. {context}"
    );
    assert_eq!(
        value("group-log"),
        "attach:GroupPage.qml|forward|",
        "attaching should happen once and the header tap should navigate \
         forward to it. {context}"
    );
    assert_eq!(
        value("single-attached"),
        "ContactPage.qml",
        "a one-to-one chat does not attach the contact's page. {context}"
    );
}
