//! Profile with several transports says which relay is its own.
//!
//! Core keeps own address (invite link, contact) in `configured_addr`. Account
//! list and connectivity report took the first transport instead: list (no
//! address since 2.61; old `addr` key held an old transport's) and report (first
//! quota bar is oldest transport's). Result: stale relay named, with a third
//! relay's mailbox.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        // Nobody's until core is up: a profile asked before that has nothing to ask.
        Profile { id: profile; account_id: 0 }
        function refresh() { core.refresh_accounts(); return 'ok' }
        function watch(id) { profile.account_id = id; return 'ok' }
        function address() { return profile.address }
        function percent() { return String(profile.quota_percent) }
        function words() { return profile.quota_text }
    }
";

#[test]
fn the_row_and_the_mailbox_are_the_profiles_own_relay() {
    let temp = std::env::temp_dir().join(format!("piirit-second-relay-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        // Set up on this relay first; own address now on another. This one is first transport
        // core lists and owns the report's first quota bar.
        std::env::set_var("PIIRIT_FAKE_OLDER_RELAY", "ada@old.example.net");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let core_ptr = std::ptr::addr_of!(core_box);

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
        call!("refresh");
        call!("watch", 1);
    });

    let listed = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let answers = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let listed_at_end = listed.clone();
    let answers_at_end = answers.clone();

    single_shot(Duration::from_secs(4), move || unsafe {
        *listed.borrow_mut() = (*core_ptr)
            .pinned()
            .borrow()
            .account_list
            .borrow()
            .iter()
            .find(|row| row.account_id == 1)
            .map_or_else(|| "<not listed>".to_string(), |row| row.addr.to_string());
        answers.borrow_mut().push(call!("address"));
        answers.borrow_mut().push(call!("percent"));
        answers.borrow_mut().push(call!("words"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let listed = listed_at_end.borrow().clone();
    let answers = answers_at_end.borrow().clone();
    let methods = common::methods(&journal);
    assert_eq!(
        listed, "account1@example.org",
        "the profiles page's row does not name the profile's own address \
         (`configured_addr`), the one its invite link and its own contact \
         carry. Calls: {methods:?}"
    );
    assert_eq!(
        answers.first().map(String::as_str),
        Some("account1@example.org"),
        "the profile page showed something other than the profile's own \
         address. Calls: {methods:?}"
    );
    assert_eq!(
        answers.get(1).map(String::as_str),
        Some("67"),
        "the mailbox shown is not this profile's relay's: the first quota \
         bar in the report belongs to the transport set up first. Calls: \
         {methods:?}"
    );
    assert_eq!(
        answers.get(2).map(String::as_str),
        Some("1.34 GiB of 2 GiB used"),
        "the mailbox words are not the ones beside this profile's relay. \
         Calls: {methods:?}"
    );
}
