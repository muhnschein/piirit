//! New-chat page, headless: real `NewChatPage` vs stub Silica. Checks tap's wire output
//! and navigation: known contact opens chat above picker. Add contact and group live
//! on chat list pull-down.

// Qt harness: `unsafe` for `env::set_var` pre-Qt. Lints: `unused_unsafe`
// (edition 2024+), `borrow_as_ptr`, `single_shot` whole secs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // `qt_method!` params match dispatcher's by-value ones; derive transmutes a QVariant
    // field: piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value,
    clippy::useless_transmute
)]

use std::path::Path;
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica `pageStack`, recorded not performed. Method names camelCase: they stand in
/// for Silica's API.
#[allow(non_snake_case)]
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `replaceAbove:ConversationPage.qml|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    /// Chat id of last navigation, or 0.
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

    /// Page below, not modelled here. `&mut self` is what `qt_method!` dispatches to.
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
            // `data` rather than `children`: the contact model is a child
            // of the page but not a visual one, and `children` holds only
            // what is drawn.
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
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // A search no contact answers to, so the list really is empty
        // and the placeholder is the only thing left to draw.
        function searchForNothing() {
            var model = findIn(loader.item, 'contacts')
            if (!model) { return 'missing:contacts' }
            model.query = 'zzqq-nobody-is-called-this'
            return 'ok'
        }
    }
";

#[test]
fn the_new_chat_page_opens_a_chat_with_a_contact() {
    let temp = std::env::temp_dir().join(format!("piirit-new-chat-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single thread; pre-Qt, pre-fork.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
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
        // Nothing read yet. Empty model: "no answer" this early, "no contacts" later.
        // Opposite meanings.
        (*steps_ptr).push((
            "placeholder-fresh",
            call!(
                "get",
                QString::from("contactsPlaceholder"),
                QString::from("enabled")
            ),
        ));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        // First known contact; tap opens chat with them.
        (*steps_ptr).push(("tap-contact", call!("click", QString::from("contactRow"))));
        (*steps_ptr).push(("search", call!("searchForNothing")));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push((
            "after-contact",
            (*stack_ptr).pinned().borrow().log.to_string(),
        ));
        // Core answered, nobody to show: placeholder should be the only item.
        (*steps_ptr).push((
            "placeholder-empty",
            call!(
                "get",
                QString::from("contactsPlaceholder"),
                QString::from("enabled")
            ),
        ));
        // No new-contact or new-group entry here: both moved to chat list pull-down; one here too
        // would be the same thing in two places.
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

/// Tapping contact ends in core-made conversation above picker; exactly one chat made.
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
            "placeholder-fresh" => assert_eq!(
                value, "false",
                "\"No contacts yet\" is up before the core has said whether \
                 there are any, so it flashes under the rows on the way in. {context}"
            ),
            "placeholder-empty" => assert_eq!(
                value, "true",
                "a search nobody answers to says nothing at all, which reads \
                 as a list that has not loaded. {context}"
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
