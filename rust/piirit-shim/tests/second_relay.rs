//! A profile with more than one transport says which relay is its own.
//!
//! The core lets a profile have several transports, keeps the one it
//! sends from in `configured_addr`, and reports on all of them at once.
//! Two places used to take whichever came first instead of that one: the
//! account list, whose `addr` is the core's deprecated key and holds the
//! address of a transport set up long ago, and the connectivity report,
//! whose first quota bar belongs to the oldest transport rather than to
//! this profile's relay. Reported from a phone whose profiles page named
//! a relay the profile had stopped sending from, under a mailbox figure
//! that belonged to a third one.

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
        // Nobody's until the core is up: a profile asked about before
        // that has nothing to ask.
        Profile { id: profile; account_id: 0 }
        function refresh() { core.refresh_accounts(); return 'ok' }
        function watch(id) { profile.account_id = id; return 'ok' }
        function address() { return profile.address }
        function percent() { return String(profile.quota_percent) }
        function words() { return profile.quota_text }
    }
";

#[test]
fn the_row_and_the_mailbox_are_the_relay_the_profile_sends_from() {
    let temp = std::env::temp_dir().join(format!("piirit-second-relay-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        // The profile was on this relay first and is not on it now: it
        // is what the core's deprecated `addr` still holds, and what the
        // first quota bar in the report belongs to.
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
        "the profiles page's row named a relay the profile no longer \
         sends from: the account list's `addr` was taken as the address \
         rather than `configured_addr`. Calls: {methods:?}"
    );
    assert_eq!(
        answers.first().map(String::as_str),
        Some("account1@example.org"),
        "the profile page showed something other than the address the \
         profile sends from. Calls: {methods:?}"
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
