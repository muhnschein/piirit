//! The page that starts a conversation, driven headlessly.
//!
//! Loads the real `NewChatPage` against the stub Silica module and the
//! recording double, and asserts what a tap produces on the wire and where
//! it navigates: a known contact opens a chat with them, above the page
//! that opened the picker. Adding a contact and making a group are not
//! here any more: both start from the chat list's pull-down.

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
    // by-value parameters, and the derive transmutes a QVariant field;
    // see piiri-shim/src/lib.rs.
    clippy::needless_pass_by_value,
    clippy::useless_transmute
)]

use std::path::Path;
use std::time::Duration;

use piiri_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`, recorded rather than performed.
///
/// Method names are camelCase because they stand in for Silica's own API.
#[allow(non_snake_case)]
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `replaceAbove:ConversationPage.qml|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    /// The chat id handed to the most recent navigation, or 0.
    chat_id: qt_property!(u32; NOTIFY log_changed),

    push: qt_method!(fn(&mut self, page: QString) -> QVariant),
    replaceAbove:
        qt_method!(fn(&mut self, target: QVariant, page: QString, properties: QVariantMap)),
    previousPage: qt_method!(fn(&mut self, page: QVariant) -> QVariant),
    pop: qt_method!(fn(&mut self, page: QVariant)),
}

#[allow(non_snake_case)]
impl PageStackProbe {
    fn note(&mut self, action: &str, page: &QString) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let current = self.log.to_string();
        self.log = format!("{current}{action}:{name}|").into();
        self.log_changed();
    }

    fn push(&mut self, page: QString) -> QVariant {
        self.note("push", &page);
        QVariant::default()
    }

    fn replaceAbove(&mut self, _target: QVariant, page: QString, properties: QVariantMap) {
        if let Some(chat_id) =
            i32::from_qvariant(properties.value(QString::from("chatId"), QVariant::default()))
        {
            self.chat_id = u32::try_from(chat_id).unwrap_or(0);
        }
        self.note("replaceAbove", &page);
    }

    /// The page below, which this record does not model. `&mut self` is
    /// what `qt_method!` dispatches to.
    #[allow(clippy::unused_self)]
    fn previousPage(&mut self, _page: QVariant) -> QVariant {
        QVariant::default()
    }

    fn pop(&mut self, _page: QVariant) {
        let current = self.log.to_string();
        self.log = format!("{current}pop|").into();
        self.log_changed();
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url, accountId) {
            loader.setSource(url, { accountId: accountId })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
    }
";

#[test]
fn the_new_chat_page_opens_a_chat_with_a_contact() {
    let temp = std::env::temp_dir().join(format!("piiri-new-chat-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piiri_shim::register_qml_types();

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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url("NewChatPage.qml")),
                1
            ),
        ));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        // The first known contact; tapping it opens a chat with them.
        (*steps_ptr).push(("tap-contact", call!("click", QString::from("contactRow"))));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push((
            "after-contact",
            (*stack_ptr).pinned().borrow().log.to_string(),
        ));
        // Neither way to something new is here: both moved to the chat
        // list's pull-down, and an entry here as well would be the same
        // thing in two places.
        (*steps_ptr).push((
            "new-contact",
            call!("click", QString::from("newContactButton")),
        ));
        (*steps_ptr).push(("new-group", call!("click", QString::from("newGroupButton"))));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(
        &steps,
        &stack_box.pinned().borrow().log.to_string(),
        stack_box.pinned().borrow().chat_id,
        &journal,
    );
}

/// Tapping a contact ends in a conversation the core made, above the
/// page that opened the picker, and made exactly one chat.
fn assert_outcome(steps: &[(&str, String)], navigation: &str, chat_id: u32, journal: &Path) {
    let context = format!("steps: {steps:?}\nnavigation: {navigation}");

    for (name, value) in steps {
        match *name {
            "after-contact" => assert_eq!(
                value, "replaceAbove:ConversationPage.qml|",
                "tapping a contact did not replace the picker with the chat. {context}"
            ),
            "new-contact" | "new-group" => assert!(
                value.starts_with("missing:"),
                "{name} is still offered here, as well as on the chat list. {context}"
            ),
            _ => assert_eq!(value, "ok", "step {name} returned {value:?}. {context}"),
        }
    }

    assert_eq!(
        navigation, "replaceAbove:ConversationPage.qml|",
        "the page navigated somewhere other than the chat. {context}"
    );
    assert!(
        chat_id > 0,
        "a conversation was opened without a chat id. {context}"
    );

    let calls = common::methods(journal);
    assert!(
        calls.iter().any(|name| name == "get_contacts"),
        "the page never asked for contacts: {calls:?}"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|name| *name == "create_chat_by_contact_id")
            .count(),
        1,
        "tapping a contact should have created exactly one chat: {calls:?}"
    );
}
