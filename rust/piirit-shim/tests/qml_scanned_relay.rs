//! A relay's code read on the QR page is a relay to add to this profile,
//! not an invite and not a profile of its own.
//!
//! The reference clients take a scanned `dcaccount:` or `dclogin:` this
//! way since 2.61. Here the code is entered on the scanner side, as
//! `qml_qr_page.rs` enters an invite: the page that adds a relay
//! replaces the QR page, handed the code and the relay it names, and
//! nothing is joined or added on the way. That page then shows the
//! relay under Delta Chat's question in place of its list, and adds it
//! -- the code as it was read -- only when its button is pressed.

// Qt harness: see qml_qr_page.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    clippy::needless_pass_by_value,
    clippy::useless_transmute
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
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
    /// What the last `replaceAbove` handed the page it made, as JSON.
    handed: qt_property!(QString; NOTIFY log_changed),

    push: qt_method!(fn(&mut self, page: QString) -> QVariant),
    replaceAbove:
        qt_method!(fn(&mut self, target: QVariant, page: QString, properties: QVariantMap)),
    previousPage: qt_method!(fn(&mut self, page: QVariant) -> QVariant),
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
        let handed: serde_json::Map<String, serde_json::Value> = properties
            .into_iter()
            .map(|(key, value)| {
                let text = QString::from_qvariant(value.clone())
                    .map(|text| text.to_string())
                    .unwrap_or_default();
                (key.to_string(), serde_json::Value::String(text))
            })
            .collect();
        self.handed = serde_json::Value::Object(handed).to_string().into();
        self.note("replaceAbove", &page);
    }

    /// The page below, which this record does not model. `&mut self` is
    /// what `qt_method!` dispatches to.
    #[allow(clippy::unused_self)]
    fn previousPage(&mut self, _page: QVariant) -> QVariant {
        QVariant::default()
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { accountId: 1 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // A page loaded with what a navigation handed it.
        function loadWith(url, json) {
            loader.setSource('', {})
            // The probe's stack keeps strings only; the profile is the
            // one this page was loaded for.
            var properties = JSON.parse(json)
            properties.accountId = 1
            loader.setSource(url, properties)
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
        function pageProperty(property) { return '' + loader.item[property] }
        // The profile row's avatar, for whose code this is.
        function profileColor() {
            var row = findIn(loader.item, 'profileRow')
            if (!row) { return 'missing:profileRow' }
            var avatar = findIn(row, 'contactAvatar')
            return avatar ? '' + avatar.ownColor : 'missing:contactAvatar'
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
    }
";

const SCANNED: &str = "DCACCOUNT:https://chat.example.org/new";

#[test]
#[allow(clippy::too_many_lines)]
fn a_scanned_relay_is_offered_for_this_profile_and_added_when_asked() {
    let temp = std::env::temp_dir().join(format!("piirit-scanned-relay-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        std::env::set_var("XDG_CACHE_HOME", &temp);
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
    let journal_ptr: *const std::path::PathBuf = std::ptr::addr_of!(journal);
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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::page_url("QrPage.qml")))
        );
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("switch", call!("click", QString::from("viewOption1")));
        record!("open", call!("click", QString::from("typeLinkButton")));
        record!(
            "typed",
            call!(
                "setText",
                QString::from("linkField"),
                QString::from(SCANNED)
            )
        );
        record!("connect", call!("click", QString::from("followButton")));
    });

    // The page it was replaced with, made with what it was handed.
    single_shot(Duration::from_secs(6), move || unsafe {
        let stack = (*stack_ptr).pinned();
        record!("navigation", stack.borrow().log.to_string());
        let handed = stack.borrow().handed.to_string();
        record!("handed", handed.clone());
        let added_early = common::methods(&*journal_ptr)
            .iter()
            .any(|name| name == "add_transport_from_qr");
        record!("added-early", added_early.to_string());
        record!(
            "relay-load",
            call!(
                "loadWith",
                QString::from(common::page_url("AddRelayPage.qml")),
                QString::from(handed)
            )
        );
        record!("intro", get!("intro", "text"));
        record!("relay-shown", get!("scannedRelay", "visible"));
        record!("relay-text", get!("scannedRelay", "text"));
        record!("list", get!("relayCombo", "visible"));
        record!("field", get!("customField", "visible"));
        record!("add-lit", get!("addButton", "enabled"));
        record!("add", call!("click", QString::from("addButton")));
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        let calls = common::calls(&*journal_ptr);
        let added = calls
            .iter()
            .find(|(name, _)| name == "add_transport_from_qr")
            .map(|(_, params)| params.to_string())
            .unwrap_or_default();
        record!("added", added);
        record!(
            "joined",
            calls
                .iter()
                .any(|(name, _)| name == "secure_join")
                .to_string()
        );
        (*engine_ptr).quit();
    });

    engine.exec();

    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps.iter().find(|(step, _)| *step == label).map_or_else(
            || panic!("no step {label}. {context}"),
            |(_, value)| value.clone(),
        )
    };
    for step in [
        "load",
        "switch",
        "open",
        "typed",
        "connect",
        "relay-load",
        "add",
    ] {
        assert_eq!(value(step), "ok", "{step} failed. {context}");
    }
    assert_eq!(
        value("navigation"),
        "replaceAbove:AddRelayPage.qml|",
        "a relay's code did not lead to the page that adds a relay. {context}"
    );
    let handed: serde_json::Value =
        serde_json::from_str(&value("handed")).expect("handed properties are JSON");
    assert_eq!(handed["scannedQr"], SCANNED, "{context}");
    assert_eq!(handed["scannedRelay"], "chat.example.org", "{context}");
    assert_eq!(
        value("added-early"),
        "false",
        "the relay was added before the reader was asked. {context}"
    );
    assert_eq!(value("intro"), "Add this relay?", "{context}");
    assert_eq!(value("relay-shown"), "true", "{context}");
    assert_eq!(value("relay-text"), "chat.example.org", "{context}");
    assert_eq!(
        (value("list").as_str(), value("field").as_str()),
        ("false", "false"),
        "the list and the field still offer another relay. {context}"
    );
    assert_eq!(value("add-lit"), "true", "{context}");
    assert_eq!(
        value("added"),
        format!("[1,\"{SCANNED}\"]"),
        "the code was not added as it was read, on this profile. {context}"
    );
    assert_eq!(
        value("joined"),
        "false",
        "a relay's code was joined. {context}"
    );
}
