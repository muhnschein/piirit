//! Deleting current profile (another left) moves app to the other; so does launch on a
//! gone profile. Bug: stayed on deleted profile, dconf remembered it so next launch resumed onto
//! it: empty list, "account with id N not found" from every page opened there (chats, settings,
//! quick-action chat picker). Quick action whose chat was in it kept pointing into it too.
//!
//! Window owns move (reasons in `qml_last_profile`): deletion lands as profiles page leaves, stack
//! mid-transition, so move held until stack can make it. Next-launch read written at once: app can
//! close before move is made.

// Qt harness: see qml_startup.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Window, loaded as `qml_last_profile` does.
fn probe_qml(components: &std::path::Path) -> String {
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import 'file://__COMPONENTS__'
    Item {
        id: probe

        // Refuses a move while a transition is running, as Silica's does,
        // and logs the profile a chat list is opened on.
        property QtObject pageStack: QtObject {
            property bool busy: false
            property string log: ''
            function name(page) { return ('' + page).split('/').pop() }
            function push(page, properties) {
                if (busy) { return }
                log += 'push:' + name(page) + '|'
            }
            function replaceAbove(target, page, properties) {
                if (busy) { return }
                log += 'replaceAbove:' + name(page) + ':'
                       + (properties ? properties.accountId : '') + '|'
            }
            function replace(page, properties) {
                if (busy) { return }
                log += 'replace:' + name(page) + '|'
            }
            function pop() {
                if (busy) { return }
                log += 'pop|'
            }
        }

        Loader { id: loader }
        function load(url) {
            loader.setSource('', {})
            loader.setSource(url, {})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // The chat list on screen is this profile's, and the phone
        // remembers it: what a chat list says when it opens.
        function onProfile(id) {
            var account = parseInt(id, 10)
            loader.item.accountId = account
            Settings.lastAccountId = account
            return 'ok'
        }
        function onScreen() { return '' + loader.item.accountId }
        function remembered() { return '' + Settings.lastAccountId }
        function setAction(side, kind, account) {
            var prefix = side === 'right' ? 'quickActionRight' : 'quickActionLeft'
            Settings[prefix] = kind
            Settings[prefix + 'Account'] = parseInt(account, 10)
            Settings[prefix + 'Chat'] = kind === 'chat' ? 1 : 0
            return 'ok'
        }
        function holds(side) {
            var prefix = side === 'right' ? 'quickActionRight' : 'quickActionLeft'
            return Settings[prefix] + '|' + Settings[prefix + 'Account'] + '|'
                   + Settings[prefix + 'Chat']
        }
        function refresh() { core.refresh_accounts(); return 'ok' }
        function deleteProfile(id) {
            core.remove_account(parseInt(id, 10))
            return 'ok'
        }
        function transition(running) {
            probe.pageStack.busy = (running === 'true')
            return 'ok'
        }
        // What the stack was asked since the last look.
        function stackLog() {
            var log = probe.pageStack.log
            probe.pageStack.log = ''
            return log
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_app_leaves_a_deleted_profile_for_one_there_is() {
    let temp = std::env::temp_dir().join(format!("piirit-deleted-profile-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub: see common::qml_tree_without_enter_key.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Two profiles: one left when other goes.
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
    }

    piirit_shim::register_qml_types();
    common::register_cover_enum();
    common::register_dbus_enum();

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
                &[$(QVariant::from(QString::from($arg))),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    let root = format!("file://{}", tree.join("piirit.qml").display());

    let mut plan = common::Plan::default();

    // 1s: app on profile 1; a quick action into each profile's chats, and one not a chat's.
    // SAFETY: callbacks run only inside `exec()`.
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("load", call!("load", root.clone())));
        (*steps_ptr).push(("on-1", call!("onProfile", "1")));
        (*steps_ptr).push(("left", call!("setAction", "left", "chat", "1")));
        (*steps_ptr).push(("right", call!("setAction", "right", "chat", "2")));
        (*steps_ptr).push(("refresh", call!("refresh")));
    });

    // 3s: both there, nothing moved. Profile 1 deleted with stack busy (as swipe back off profiles
    // page leaves it).
    plan.after(2, move || unsafe {
        (*steps_ptr).push(("kept", call!("stackLog")));
        (*steps_ptr).push(("left-kept", call!("holds", "left")));
        (*steps_ptr).push(("transition", call!("transition", "true")));
        (*steps_ptr).push(("delete", call!("deleteProfile", "1")));
    });

    // 5s: move held; next-launch read not.
    plan.after(2, move || unsafe {
        (*steps_ptr).push(("held", call!("stackLog")));
        (*steps_ptr).push(("remembered", call!("remembered")));
        (*steps_ptr).push(("window", call!("onScreen")));
        (*steps_ptr).push(("left-gone", call!("holds", "left")));
        (*steps_ptr).push(("right-kept", call!("holds", "right")));
        (*steps_ptr).push(("settled", call!("transition", "false")));
    });

    // 6s: it lands. Then launch resumes onto profile core lacks (stale or deleted elsewhere).
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("landed", call!("stackLog")));
        (*steps_ptr).push(("stale", call!("onProfile", "9")));
        (*steps_ptr).push(("stale-refresh", call!("refresh")));
    });

    plan.after(2, move || unsafe {
        (*steps_ptr).push(("stale-landed", call!("stackLog")));
        (*steps_ptr).push(("stale-remembered", call!("remembered")));
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

    assert_eq!(value("load"), "ok", "the window does not load. {context}");
    assert_eq!(
        value("kept"),
        "",
        "a list with the profile on screen in it moved the stack. {context}"
    );
    assert_eq!(
        value("left-kept"),
        "chat|1|1",
        "a quick action into a profile that is there was dropped. {context}"
    );
    assert_eq!(
        value("held"),
        "",
        "the move was asked for while the stack was still transitioning, \
         which is where Silica drops it. {context}"
    );
    assert_eq!(
        value("remembered"),
        "2",
        "the phone still remembers the deleted profile, so the next launch \
         resumes onto a chat list the core cannot open. {context}"
    );
    assert_eq!(
        value("window"),
        "2",
        "a share would still go into the deleted profile. {context}"
    );
    assert_eq!(
        value("left-gone"),
        "|0|0",
        "a quick action still opens a chat in the deleted profile. {context}"
    );
    assert_eq!(
        value("right-kept"),
        "chat|2|1",
        "a quick action into the profile that is left was dropped. {context}"
    );
    assert_eq!(
        value("landed"),
        "replaceAbove:ChatListPage.qml:2|",
        "the app is still on the deleted profile's chat list -- empty, and \
         \"account with id 1 not found\" from every page opened from it. \
         {context}"
    );
    assert_eq!(
        value("stale-landed"),
        "replaceAbove:ChatListPage.qml:2|",
        "a launch onto a profile the core does not have stays on it. \
         {context}"
    );
    assert_eq!(value("stale-remembered"), "2", "{context}");
}
