//! The QR code page: this profile's invite drawn as a code on one side
//! of the switch, the scanner on the other.
//!
//! Loaded headlessly against the stub Silica module and the recording
//! double: once the invite arrives the code has modules and the picture
//! is shown from a file; the switch brings the scanner up, loaded from
//! its own file; the button under the viewfinder opens the link panel,
//! and a link entered there is followed into a chat that replaces this
//! page above the one that opened it. Nothing is pushed: the scanner is
//! a side of this page, not a page of its own.

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
    // see piirit-shim/src/lib.rs.
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

    fn replaceAbove(&mut self, _target: QVariant, page: QString, _properties: QVariantMap) {
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

const TYPED: &str = "https://i.delta.chat/#FEDCBA9876543210&a=typed%40example.org&n=Typed";

#[test]
#[allow(clippy::too_many_lines)]
fn the_page_shows_the_invite_and_follows_one_entered_on_the_scanner_side() {
    let temp = std::env::temp_dir().join(format!("piirit-qr-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
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
        // The code side is up first, and the invite has arrived from the
        // core and is drawn.
        record!("mode", call!("pageProperty", QString::from("mode")));
        record!("code-side", get!("codeView", "visible"));
        record!("scan-side", get!("scanArea", "visible"));
        // Whose code: the profile, in its own colour, above it.
        record!("profile-row", get!("profileRow", "visible"));
        record!("profile-color", call!("profileColor"));
        record!("invite", get!("myInviteLabel", "text"));
        record!("code-size", get!("qr", "size"));
        record!("code-shown", get!("inviteQr", "visible"));
        record!("code-image", get!("inviteQr", "source"));
        // Nothing to paste into on this side: the field is the scanner's.
        record!("field-before", get!("linkField", "visible"));
        // The switch: the scanner side, loaded from its own file.
        record!("switch", call!("click", QString::from("viewOption1")));
        record!("mode-after", call!("pageProperty", QString::from("mode")));
        record!("code-side-after", get!("codeView", "visible"));
        record!("scan-side-after", get!("scanArea", "visible"));
        record!("scanner-loaded", get!("scanLoader", "status"));
        record!("viewfinder", get!("viewfinder", "visible"));
        // The link, entered under the viewfinder and followed.
        record!("panel-before", get!("linkPanel", "visible"));
        record!("button", get!("typeLinkButton", "visible"));
        record!("open", call!("click", QString::from("typeLinkButton")));
        record!("panel-open", get!("linkPanel", "visible"));
        record!("button-after", get!("typeLinkButton", "visible"));
        record!(
            "typed",
            call!("setText", QString::from("linkField"), QString::from(TYPED))
        );
        record!("connect", call!("click", QString::from("followButton")));
        record!("acting", get!("acting", "running"));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("navigation", (*stack_ptr).pinned().borrow().log.to_string());
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_page(&steps, &common::methods(&journal));
}

/// The code is the invite; the switch brings the scanner up; a link
/// entered there is followed into a chat over this page.
#[allow(clippy::too_many_lines)]
fn assert_page(steps: &[(&str, String)], calls: &[String]) {
    let context = format!("steps: {steps:?}\ncalls: {calls:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    for label in ["load", "switch", "open", "typed", "connect"] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert_eq!(
        value("mode"),
        "0",
        "the page did not open on the code. {context}"
    );
    assert_eq!(
        value("code-side"),
        "true",
        "the code side is not showing at first. {context}"
    );
    assert_eq!(
        value("scan-side"),
        "false",
        "the scanner side is showing beside the code. {context}"
    );
    assert_eq!(
        value("profile-row"),
        "true",
        "the code side does not say whose code it is. {context}"
    );
    assert_eq!(
        value("profile-color"),
        "#00875a",
        "the profile row is not drawn in the colour the core gives the \
         reader's own contact. {context}"
    );
    assert!(
        value("invite").starts_with("https://i.delta.chat/"),
        "the invite never arrived. {context}"
    );
    let size: u32 = value("code-size").parse().unwrap_or(0);
    assert!(
        size >= 21,
        "the invite was not encoded as a code of at least version 1. {context}"
    );
    assert_eq!(
        value("code-shown"),
        "true",
        "the code is not shown once there is one. {context}"
    );
    let image = value("code-image");
    assert!(
        image.starts_with("file://")
            && std::path::Path::new(&image)
                .extension()
                .is_some_and(|ext| ext == "pgm"),
        "the code is not shown from a file the shim wrote: {image:?}. {context}"
    );
    assert_eq!(
        value("field-before"),
        "missing:linkField",
        "the code side has a field to paste a link into; that is the \
         scanner's. {context}"
    );
    assert_eq!(
        value("mode-after"),
        "1",
        "the switch did not switch. {context}"
    );
    assert_eq!(
        value("code-side-after"),
        "false",
        "the code is still showing under the scanner. {context}"
    );
    assert_eq!(
        value("scan-side-after"),
        "true",
        "the scanner side did not come up. {context}"
    );
    // Loader.Ready.
    assert_eq!(
        value("scanner-loaded"),
        "1",
        "the scanner did not load from its own file. {context}"
    );
    assert_eq!(
        value("viewfinder"),
        "true",
        "the scanner side has no viewfinder. {context}"
    );
    assert_eq!(
        value("panel-before"),
        "false",
        "the link panel is up before anyone asked for it. {context}"
    );
    assert_eq!(
        value("button"),
        "true",
        "the way to a typed link is not a button under the viewfinder. {context}"
    );
    assert_eq!(
        value("panel-open"),
        "true",
        "the button did not open the link panel. {context}"
    );
    assert_eq!(
        value("button-after"),
        "false",
        "the button stays up over the panel it opened. {context}"
    );
    assert_eq!(
        value("acting"),
        "true",
        "the scanner does not show that the link is being acted on. {context}"
    );
    assert!(
        calls.iter().any(|name| name == "secure_join"),
        "the entered link was not followed. {context}"
    );
    // Above the page below this one, and nothing pushed on the way: the
    // scanner is a side of this page, and the chat replaces the page.
    assert_eq!(
        value("navigation"),
        "replaceAbove:ConversationPage.qml|",
        "following the link did not open the chat over this page. {context}"
    );
}
