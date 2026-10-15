//! Event burst on open chat is not a burst of reads.
//!
//! Was: each `IncomingMsg` re-read whole id list, each receipt/reaction its message; catch-up after
//! a night offline = full chat read per message. Pinned: events during a read cost that read + one
//! more, which sees them all.

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

/// Events per kind in burst.
const BURST: u32 = 10;

// Events go straight to model, not via core.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        function burst(times) {
            for (var i = 0; i < times; i++) {
                chat.handle_event(1, 'IncomingMsg', JSON.stringify({chatId: 1, msgId: 2}))
                chat.handle_event(1, 'MsgDelivered', JSON.stringify({chatId: 1, msgId: 2}))
            }
        }
        function count() { return chat.count }
    }
";

#[test]
fn a_burst_of_events_is_one_read_and_one_more() {
    let temp = std::env::temp_dir().join(format!("piirit-sync-coalesce-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Long: burst lands during first reads. Short: later reads done before count.
        std::env::set_var("PIIRIT_FAKE_FETCH_DELAY_MS", "300");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and `engine` outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*engine_ptr).invoke_method("burst".into(), &[QVariant::from(BURST)]);
    });

    let mut count = 0_u32;
    let count_ptr: *mut u32 = std::ptr::addr_of_mut!(count);
    single_shot(Duration::from_secs(6), move || unsafe {
        let value = (*engine_ptr).invoke_method("count".into(), &[]);
        *count_ptr = u32::from_qvariant(value).unwrap_or_default();
        (*engine_ptr).quit();
    });

    engine.exec();

    let names = common::methods(&journal);
    let reads = |method: &str| names.iter().filter(|name| *name == method).count();

    assert_eq!(
        count, 2,
        "the chat did not open with its two messages: {names:?}"
    );
    // One to open chat, one for burst's first event, one after for rest. Old: one per event.
    assert_eq!(
        reads("get_message_list_items"),
        3,
        "the burst was not one read of the id list and one more: {names:?}"
    );
    // Same for the message all delivery reports named: open page, re-read in air, one after.
    // Id-list reads found nothing new, so fetched no messages.
    assert_eq!(
        reads("get_messages"),
        3,
        "the burst was not one re-read of the message and one more: {names:?}"
    );
}
