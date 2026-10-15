//! Profile setup page on a relay that keeps reader waiting.
//!
//! `qml_pages.rs` uses instant relay. This is the double's `slow` relay: volunteers'-relays
//! hint appears under Cancel at 4th second; at deadline, time-out with relay name and way back;
//! Cancel on a second attempt (begun while core on first) stops and goes back.

// Qt harness: `unsafe` `env::set_var` before Qt starts (`unused_unsafe`: unsafe only from
// edition 2024), `borrow_as_ptr` for engine pointer, `single_shot` with whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! params match dispatcher; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::cell::RefCell;
use std::ffi::CString;
use std::rc::Rc;
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// `BusyIndicatorSize.Large`, which `.qml` stub cannot give.
#[derive(QEnum)]
#[repr(u8)]
enum BusyIndicatorSize {
    Small = 0,
    Medium = 1,
    Large = 2,
}

/// Records navigation, not performed. `camelCase`: Silica's own API.
#[allow(non_snake_case)]
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `pop|replaceAbove:ChatListPage.qml|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),

    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    replaceAbove:
        qt_method!(fn(&mut self, target: QVariant, page: QString, properties: QVariantMap)),
    pop: qt_method!(fn(&mut self)),
}

#[allow(non_snake_case)]
impl PageStackProbe {
    fn record(&mut self, entry: &str) {
        let current = self.log.to_string();
        self.log = format!("{current}{entry}|").into();
        self.log_changed();
    }

    fn name(page: &QString) -> String {
        let page = page.to_string();
        page.rsplit('/').next().unwrap_or(&page).to_string()
    }

    fn push(&mut self, page: QString, _properties: QVariantMap) {
        self.record(&format!("push:{}", Self::name(&page)));
    }

    fn replaceAbove(&mut self, _target: QVariant, page: QString, _properties: QVariantMap) {
        self.record(&format!("replaceAbove:{}", Self::name(&page)));
    }

    fn pop(&mut self) {
        self.record("pop");
    }
}

/// Owns `Loader`; walks page children by `objectName`.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: root
        Loader { id: loader }

        // A fresh page each time, with what the dialog hands it, made
        // before it is on screen as Silica makes a destination.
        function loadWith(url, json) {
            loader.setSource('', {})
            loader.setSource(url, JSON.parse(json))
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // What Silica does when a page becomes the one on screen.
        function activate() { loader.item.status = 2; return 'ok' }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
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
        function pageProperty(property) {
            return loader.item ? '' + loader.item[property] : 'no-page'
        }
    }
";

const SLOW_PAGE: &str = r#"{"displayName":"Ada","providerQr":"dcaccount:slow.example","status":0}"#;

/// Each step and what it recorded.
// Engine, QObjects, steps: one scope, must outlive `exec()`.
#[allow(clippy::too_many_lines)]
#[test]
fn a_slow_relay_is_explained_given_up_on_and_cancellable() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-setup-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // SAFETY: single-threaded; set before Qt and server start.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Relay that does not answer while watched.
        std::env::set_var("PIIRIT_FAKE_SLOW_MS", "20000");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(PageStackProbe::default());

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    let uri = CString::new("Sailfish.Silica").expect("static uri");
    qml_register_enum::<BusyIndicatorSize>(
        &uri,
        1,
        0,
        &CString::new("BusyIndicatorSize").expect("static name"),
    );
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    // Six seconds, not thirty: page under test, not the wait.
    core_box.pinned().borrow_mut().profile_timeout = 6;
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let steps: common::Steps = Rc::new(RefCell::new(Vec::new()));
    let engine_ptr = std::ptr::addr_of_mut!(engine);

    // SAFETY: callbacks run only inside `exec()`; `engine` outlives them.
    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = unsafe {
                (*engine_ptr).invoke_method(
                    $name.into(),
                    &[$(QVariant::from(QString::from($arg))),*],
                )
            };
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }

    // 1s: page on screen asks slow relay.
    let s = steps.clone();
    single_shot(Duration::from_secs(1), move || {
        common::record(
            &s,
            "load",
            call!(
                "loadWith",
                common::page_url("ProfileSetupPage.qml"),
                SLOW_PAGE
            ),
        );
        common::record(&s, "activate", call!("activate"));
    });

    // 2s: waiting, nothing said yet.
    let s = steps.clone();
    single_shot(Duration::from_secs(2), move || {
        common::record(&s, "early-busy", call!("pageProperty", "busy"));
        common::record(&s, "early-hint", call!("get", "slowHint", "visible"));
        common::record(&s, "early-label", call!("get", "progressBar", "label"));
    });

    // 6s: fourth second passed.
    let s = steps.clone();
    single_shot(Duration::from_secs(6), move || {
        common::record(&s, "hint", call!("get", "slowHint", "visible"));
        common::record(&s, "still-busy", call!("pageProperty", "busy"));
    });

    // 8s: deadline passed at seven.
    let s = steps.clone();
    single_shot(Duration::from_secs(8), move || {
        common::record(&s, "late-busy", call!("pageProperty", "busy"));
        common::record(&s, "late-error", call!("pageProperty", "errorMessage"));
        common::record(&s, "late-hint", call!("get", "slowHint", "visible"));
        common::record(&s, "late-back", call!("get", "backButton", "visible"));
        common::record(&s, "late-cancel", call!("get", "cancelButton", "visible"));
    });

    // 9s: second attempt, core still on first.
    let s = steps.clone();
    single_shot(Duration::from_secs(9), move || {
        common::record(
            &s,
            "reload",
            call!(
                "loadWith",
                common::page_url("ProfileSetupPage.qml"),
                SLOW_PAGE
            ),
        );
        common::record(&s, "reactivate", call!("activate"));
    });

    // 10s: cancelled.
    let s = steps.clone();
    single_shot(Duration::from_secs(10), move || {
        common::record(&s, "cancel", call!("click", "cancelButton"));
        common::record(&s, "cancelled-busy", call!("pageProperty", "busy"));
    });

    single_shot(Duration::from_secs(12), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow();
    let navigation = stack_box.pinned().borrow().log.to_string();
    let calls = common::calls(&journal);
    let context = format!("steps: {steps:?}\nnavigation: {navigation}\ncalls: {calls:?}");

    assert_hint_timing(&steps, &context);
    assert_time_out(&steps, &context);
    assert_cancel(&steps, &navigation, &calls, &context);
}

/// Hint absent at two seconds, present at six.
fn assert_hint_timing(steps: &[(String, String)], context: &str) {
    for step in ["load", "activate"] {
        assert_eq!(
            common::value_of(steps, step),
            "ok",
            "{step} failed. {context}"
        );
    }
    assert_eq!(common::value_of(steps, "early-busy"), "true", "{context}");
    assert_eq!(
        common::value_of(steps, "early-hint"),
        "false",
        "the hint about relays was shown before the relay had kept anyone \
         waiting. {context}"
    );
    assert_eq!(
        common::value_of(steps, "early-label"),
        "Contacting slow.example…",
        "the progress bar does not name the relay. {context}"
    );
    assert_eq!(
        common::value_of(steps, "hint"),
        "true",
        "four seconds of waiting did not bring the hint up. {context}"
    );
    assert_eq!(common::value_of(steps, "still-busy"), "true", "{context}");
}

/// At deadline: stops waiting, names silent relay and wait, offers way back; hint stays.
fn assert_time_out(steps: &[(String, String)], context: &str) {
    assert_eq!(
        common::value_of(steps, "late-busy"),
        "false",
        "the page is still waiting after the deadline. {context}"
    );
    assert_eq!(
        common::value_of(steps, "late-error"),
        "slow.example did not answer within 6 seconds.",
        "the time-out is not said with the relay and the seconds. {context}"
    );
    assert_eq!(
        common::value_of(steps, "late-hint"),
        "true",
        "the hint went with the wait; it is the way it points at. {context}"
    );
    assert_eq!(common::value_of(steps, "late-back"), "true", "{context}");
    assert_eq!(common::value_of(steps, "late-cancel"), "false", "{context}");
}

/// Second attempt takes fresh account; Cancel stops held and goes back.
fn assert_cancel(
    steps: &[(String, String)],
    navigation: &str,
    calls: &[(String, Value)],
    context: &str,
) {
    for step in ["reload", "reactivate", "cancel"] {
        assert_eq!(
            common::value_of(steps, step),
            "ok",
            "{step} failed. {context}"
        );
    }
    assert_eq!(
        common::value_of(steps, "cancelled-busy"),
        "false",
        "{context}"
    );
    assert!(
        navigation.ends_with("pop|"),
        "Cancel did not go back. {context}"
    );
    let added = calls
        .iter()
        .filter(|(method, _)| method == "add_account")
        .count();
    assert_eq!(
        added, 2,
        "the second attempt did not take a fresh account while the core \
         was still on the first. {context}"
    );
    assert!(
        calls
            .iter()
            .filter(|(method, _)| method == "stop_ongoing_process")
            .any(|(_, params)| params.get(0).and_then(Value::as_u64) == Some(2)),
        "Cancel did not stop the process on the account it was holding. {context}"
    );
}
