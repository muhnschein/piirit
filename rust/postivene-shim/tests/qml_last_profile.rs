//! The last profile going takes the app back to the welcome page.
//!
//! A chat list left open on an account the core no longer has is a page
//! that can read nothing and write nothing, and answers a tap on a chat
//! with "account with id N not found". So an account list that comes
//! back empty is a way back to the first screen.
//!
//! The move belongs to the window rather than to any page, and both
//! halves of that are what this pins.
//!
//! It cannot be a page's, because of when it happens: the reader deletes
//! from the profiles page and swipes back, so the deletion lands while
//! the profiles page is being destroyed and the chat list under it is
//! mid-transition. The page that asked is gone, and Silica refuses a
//! move asked for during a transition -- on stderr, and nowhere the
//! reader will see. What was left was a chat list showing chats of a
//! profile that no longer existed, with a profiles page behind it
//! showing none. Both pages carrying the same handler was the other half
//! of it: the one time they were both alive to hear it, they both asked.
//!
//! And it cannot fire on every empty list, or a phone that has never had
//! a profile would be sent to the welcome page it is already looking at,
//! from under the reader's first tap.

// Qt harness: see qml_startup.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    non_snake_case,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see postivene-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// A page stack that refuses a move while a transition is running, as
/// Silica's does.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    busy: qt_property!(bool; NOTIFY busy_changed),
    busy_changed: qt_signal!(),

    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    replaceAbove:
        qt_method!(fn(&mut self, target: QVariant, page: QString, properties: QVariantMap)),
    replace: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    pop: qt_method!(fn(&mut self)),
}

#[allow(non_snake_case)]
impl PageStackProbe {
    fn record(&mut self, entry: &str) {
        if self.busy {
            return;
        }
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

    fn replace(&mut self, page: QString, _properties: QVariantMap) {
        self.record(&format!("replace:{}", Self::name(&page)));
    }

    fn pop(&mut self) {
        self.record("pop");
    }
}

/// The window, loaded the way `qml_startup` loads it: from a copy of the
/// tree, with the components directory imported so the probe can reach
/// the `Settings` singleton the window reads.
fn probe_qml(components: &std::path::Path) -> String {
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import 'file://__COMPONENTS__'
    Item {
        Loader { id: loader }
        function remember(id) {
            Settings.lastAccountId = parseInt(id, 10)
            return 'ok'
        }
        function load(url) {
            loader.setSource('', {})
            loader.setSource(url, {})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function remembered() { return '' + Settings.lastAccountId }
        function makeProfile() {
            core.create_profile('Ada', 'dcaccount:nine.testrun.org')
            return 'ok'
        }
        function refresh() { core.refresh_accounts(); return 'ok' }
        function deleteProfile(id) {
            core.remove_account(parseInt(id, 10))
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_app_goes_back_to_the_first_screen_when_the_last_profile_is_gone() {
    let temp = std::env::temp_dir().join(format!("postivene-last-profile-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub; the copy keeps this test to one reason to
    // fail. See common::qml_tree_without_enter_key.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(PageStackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    // The real one is handed in by main.rs; the value is never read here.
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
    let stack_ptr = std::ptr::addr_of!(stack_box);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from(QString::from($arg))),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }
    macro_rules! transition {
        ($running:expr) => {{
            let stack = (*stack_ptr).pinned();
            stack.borrow_mut().busy = $running;
            stack.borrow().busy_changed();
        }};
    }
    macro_rules! log {
        () => {
            (*stack_ptr).pinned().borrow().log.to_string()
        };
    }

    let root = format!("file://{}", tree.join("postivene.qml").display());

    // 1s: a phone that has never had a profile. dconf outlives the test
    // binary, so the key is set rather than assumed.
    // SAFETY: the callbacks fire only while `exec()` runs on this thread.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push(("forget", call!("remember", "0")));
        (*steps_ptr).push(("load", call!("load", root.clone())));
        (*steps_ptr).push(("refresh", call!("refresh")));
    });

    // 3s: nothing was moved for the empty list it started with, and a
    // profile is made.
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("fresh", log!()));
        (*steps_ptr).push(("make", call!("makeProfile")));
    });

    // 5s: the list is read again, the way every arrival on the chat list
    // reads it. That is the window learning this phone has a profile.
    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("reread", call!("refresh")));
    });

    // 6s: it is deleted with the stack busy, which is what a swipe back
    // off the profiles page leaves behind.
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("before", log!()));
        transition!(true);
        (*steps_ptr).push(("delete", call!("deleteProfile", "1")));
    });

    // 8s: the move has not been made -- and is not lost either.
    single_shot(Duration::from_secs(8), move || unsafe {
        (*steps_ptr).push(("held", log!()));
        transition!(false);
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        (*steps_ptr).push(("landed", log!()));
        (*steps_ptr).push(("remembered", call!("remembered")));
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

    assert_eq!(value("load"), "ok", "the window does not load. {context}");
    assert_eq!(
        value("fresh"),
        "",
        "a phone that has never had a profile was sent to the welcome \
         page it is already showing. {context}"
    );
    assert_eq!(
        value("before"),
        "",
        "a list with a profile in it moved the stack. {context}"
    );
    assert_eq!(
        value("held"),
        "",
        "the way back was asked for while the stack was still \
         transitioning, which is where Silica drops it. {context}"
    );
    assert_eq!(
        value("landed"),
        "replaceAbove:WelcomePage.qml|",
        "the last profile is gone and the app is still on the chat list \
         of it -- a page that cannot read or write anything, and answers \
         a tap on a chat with the core's \"account not found\". {context}"
    );
    assert_eq!(
        value("remembered"),
        "0",
        "the phone still remembers a profile that is gone, so the next \
         launch resumes onto a chat list the core cannot open. {context}"
    );
}
