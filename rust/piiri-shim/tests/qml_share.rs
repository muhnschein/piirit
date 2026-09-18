//! Something shared to the app from somewhere else on the phone.
//!
//! The platform offers this app in its share sheet because the desktop
//! file says so, and hands what was shared to a `ShareProvider` running
//! inside it (`qml/share/ShareTarget.qml`). What this pins is the part
//! that is ours: that a file and a piece of text are taken out of what
//! arrives, that they reach the window, that the window asks which chat
//! they are for -- and only when there is a profile to ask about -- and
//! that the chosen chat is opened carrying them.
//!
//! The provider is a stub, so nothing here says the phone really offers
//! the app. That is a device question; `docs/HARBOUR.md` says how to try
//! it.

// Qt harness: see qml_startup.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use piiri_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`, recorded rather than performed, with the
/// properties each page was given: what a share hands over travels as
/// those.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap) -> QVariant),
    replace: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    pop: qt_method!(fn(&mut self)),
}

impl StackProbe {
    /// The page and the properties it was given, in one line.
    ///
    /// Read by name and by the type each one is: a `QVariantMap` has no
    /// iteration here, and these are the properties a share travels as.
    fn note(&mut self, what: &str, page: &QString, properties: &QVariantMap) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let mut shown: Vec<String> = Vec::new();
        let mut read = |key: &str, as_text: &dyn Fn(QVariant) -> Option<String>| {
            if properties.contains(key.into()) {
                let value = properties.value(key.into(), QVariant::default());
                if let Some(text) = as_text(value) {
                    shown.push(format!("{key}={text}"));
                }
            }
        };
        let number = |value: QVariant| u32::from_qvariant(value).map(|found| found.to_string());
        let text = |value: QVariant| QString::from_qvariant(value).map(|found| found.to_string());
        let flag = |value: QVariant| bool::from_qvariant(value).map(|found| found.to_string());
        read("accountId", &number);
        read("chatId", &number);
        read("chatName", &text);
        read("closeOnPick", &flag);
        read("sharedFile", &text);
        read("sharedText", &text);
        let current = self.log.to_string();
        self.log = format!("{current}{what}:{name}{{{}}};", shown.join(",")).into();
        self.log_changed();
    }

    fn push(&mut self, page: QString, properties: QVariantMap) -> QVariant {
        self.note("push", &page, &properties);
        QVariant::default()
    }

    fn replace(&mut self, page: QString, properties: QVariantMap) {
        self.note("replace", &page, &properties);
    }

    fn pop(&mut self) {
        let current = self.log.to_string();
        self.log = format!("{current}pop;").into();
        self.log_changed();
    }
}

/// The root stands in for the window while the share target is loaded on
/// its own: a component a `Loader` builds reads ids from the file that
/// loaded it, which is how `ShareTarget.qml` reaches `appWindow` inside
/// the real one.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: appWindow

        property string log: ''
        property int accountId: 0

        function activate() { log += 'activate;' }
        function shareInto(filePath, text) {
            log += 'share:' + filePath + '|' + text + ';'
        }

        Loader { id: target }
        Loader { id: window }

        function loadTarget(url) {
            target.setSource(url, {})
            return target.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function loadWindow(url) {
            window.setSource(url, {})
            return window.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        // The platform handing something over, as the provider reports
        // it: a file by path, a piece of text as data.
        function shareFile(path) {
            var provider = findIn(target.item, 'shareFiles')
            if (!provider) { return 'missing:shareFiles' }
            provider.triggered([{ type: 0, filePath: path }])
            return 'ok'
        }
        function shareText(words) {
            var provider = findIn(target.item, 'shareText')
            if (!provider) { return 'missing:shareText' }
            provider.triggered([{ type: 1, data: words }])
            return 'ok'
        }
        function shareNothing() {
            var provider = findIn(target.item, 'shareFiles')
            if (!provider) { return 'missing:shareFiles' }
            provider.triggered([])
            return 'ok'
        }
        function said() { var out = log; log = ''; return out }
        // The window's own side of it.
        function setAccount(id) { window.item.accountId = id; return 'ok' }
        function intoWindow(path, words) {
            window.item.shareInto(path, words)
            return 'ok'
        }
        function openIt(chatId, name, path, words) {
            window.item.openShared(chatId, name, path, words)
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn what_is_shared_reaches_the_chat_the_reader_picks() {
    let temp = std::env::temp_dir().join(format!("piiri-qml-share-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub; the window reaches pages that use it.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piiri_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(StackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    common::register_dbus_enum();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.set_property(
        "rpcServerPath".into(),
        QString::from(env!("CARGO_BIN_EXE_fake-core-server")).into(),
    );
    engine.load_data(QByteArray::from(PROBE_QML));

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

    let target = format!("file://{}", tree.join("share/ShareTarget.qml").display());
    let window = format!("file://{}", tree.join("piiri.qml").display());

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load-target",
            call!("loadTarget", QString::from(target.clone()))
        );
        record!(
            "file",
            call!("shareFile", QString::from("/tmp/piiri-shared.png"))
        );
        record!("file-said", call!("said"));
        record!("text", call!("shareText", QString::from("hello there")));
        record!("text-said", call!("said"));
        // Nothing in it: nothing to open a chat for.
        record!("empty", call!("shareNothing"));
        record!("empty-said", call!("said"));

        record!(
            "load-window",
            call!("loadWindow", QString::from(window.clone()))
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        // No profile yet -- the app is still on its way in -- so there is
        // no chat to ask about.
        record!(
            "no-profile",
            call!(
                "intoWindow",
                QString::from("/tmp/piiri-shared.png"),
                QString::from("")
            )
        );
        record!(
            "no-profile-stack",
            (*stack_ptr).pinned().borrow().log.to_string()
        );
        record!("account", call!("setAccount", 7));
        record!(
            "into",
            call!(
                "intoWindow",
                QString::from("/tmp/piiri-shared.png"),
                QString::from("")
            )
        );
        record!(
            "opened",
            call!(
                "openIt",
                3,
                QString::from("Ada"),
                QString::from("/tmp/piiri-shared.png"),
                QString::from("")
            )
        );
        record!("stack", (*stack_ptr).pinned().borrow().log.to_string());
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
        value("load-target"),
        "ok",
        "the share target did not load. {context}"
    );
    assert_eq!(
        value("file-said"),
        "activate;share:/tmp/piiri-shared.png|;",
        "a shared file did not reach the window. {context}"
    );
    assert_eq!(
        value("text-said"),
        "activate;share:|hello there;",
        "shared text did not reach the window. {context}"
    );
    assert_eq!(
        value("empty-said"),
        "",
        "a share carrying nothing still opened the app. {context}"
    );

    assert_eq!(
        value("load-window"),
        "ok",
        "the window did not load. {context}"
    );
    assert_eq!(
        value("no-profile-stack"),
        "",
        "a share asked which chat to open before there was a profile to \
         ask about. {context}"
    );
    let stack = value("stack");
    assert!(
        stack.contains("push:ChatPickerPage.qml{accountId=7,closeOnPick=false}"),
        "the share did not ask which chat it was for, with the picker \
         left standing for the chat to replace: {stack}. {context}"
    );
    assert!(
        stack.contains(
            "replace:ConversationPage.qml{accountId=7,chatId=3,chatName=Ada,\
             sharedFile=/tmp/piiri-shared.png,sharedText=}"
        ),
        "the chosen chat was not opened carrying what was shared: {stack}. \
         {context}"
    );
}
