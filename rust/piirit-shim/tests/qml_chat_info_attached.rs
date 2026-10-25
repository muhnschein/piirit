//! Chat info attached right of conversation: page indicator shows it, swipe reaches it (platform's
//! "more this way"). Was header-tap only, which nothing signalled. Page depends on chat kind, so
//! conversation waits for load; header tap now goes the way the swipe does.
//!
//! Page loaded from QML tree copy with `EnterKey` taken out; see
//! `common::qml_tree_without_enter_key`.

// Qt harness: `unsafe` env::set_var pre-Qt (`unused_unsafe`: unsafe only from edition 2024),
// `borrow_as_ptr` for engine pointer.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::expect_used,
    // `qt_method!` params match dispatcher (`piirit-shim/src/lib.rs`).
    clippy::needless_pass_by_value
)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`, recorded not performed: what was attached, whether forward nav was taken.
// Named as QML calls them: qmetaobject exposes methods under Rust name; Silica's are camel-cased.
#[allow(non_snake_case)]
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `attach:GroupPage.qml|forward|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    /// Page named by last `pushAttached`; empty until one.
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
        // No page handed back: conversation only connects to a group's `renamed`; null says nothing
        // to connect to, as Silica does for a page that failed to load.
        QVariant::default()
    }

    /// Whether something is attached. Silica answers with the page; conversation only asks if any.
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

    // SAFETY: single-threaded; set pre-Qt.
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
    // Read from steps like engine: box outlives `exec()`; closures fire only while it runs.
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

    let mut plan = common::Plan::default();

    // Group, chat 2, loaded while arriving: nothing attachable yet.
    let group_url = common::page_url_in(&tree, "ConversationPage.qml");
    plan.after(2, move || unsafe {
        record!(
            "load-group",
            call!("load", QString::from(group_url.clone()), 2)
        );
    });
    plan.after(2, move || unsafe {
        record!("group-loaded", call!("loaded"));
        record!(
            "before-active",
            (*stack_ptr).pinned().borrow().log.to_string()
        );
        record!("settle-group", call!("settle"));
    });
    plan.after(1, move || unsafe {
        record!(
            "group-attached",
            (*stack_ptr).pinned().borrow().attached.to_string()
        );
        // Settling again must not attach a second.
        record!("resettle", call!("settle"));
        record!("tap", call!("tapHeader"));
        record!("group-log", (*stack_ptr).pinned().borrow().log.to_string());
        // Now one-to-one chat (chat 1), fresh stack.
        (*stack_ptr).pinned().borrow_mut().attached = QString::default();
    });
    let single_url = common::page_url_in(&tree, "ConversationPage.qml");
    plan.after(1, move || unsafe {
        record!(
            "load-single",
            call!("load", QString::from(single_url.clone()), 1)
        );
    });
    plan.after(2, move || unsafe {
        record!("settle-single", call!("settle"));
    });
    plan.after(1, move || unsafe {
        record!(
            "single-attached",
            (*stack_ptr).pinned().borrow().attached.to_string()
        );
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    assert_attached(&steps);
}

/// Right page for chat kind: attached once; header tap too.
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
