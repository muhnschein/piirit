//! A burst of events for the open chat is not a burst of reads.
//!
//! Every `IncomingMsg` read the chat's whole id list again, and every
//! delivery report or reaction re-read its message, each on its own and
//! all at once: the catch-up after a night offline in a busy group was a
//! full read of the chat per message, each diffed on the Qt thread as it
//! landed. What is pinned here is that however many events arrive while a
//! read is in the air, they cost that read and one more after it -- the
//! one that is sure to see what they announced.

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

/// How many of each event the burst is.
const BURST: u32 = 10;

// The events are handed to the model directly rather than through the
// core, so nothing but the burst reaches it.
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

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Long enough that the whole burst lands while the first reads
        // are still in the air, and short enough that the reads after
        // them are done well before the count is taken.
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
    // One to open the chat, one for the first event of the burst, and one
    // after it for the rest. Each event reading the list for itself was
    // one per event.
    assert_eq!(
        reads("get_message_list_items"),
        3,
        "the burst was not one read of the id list and one more: {names:?}"
    );
    // The same for the one message every delivery report named: the
    // opening page, then one re-read in the air and one after it. The
    // id-list reads found nothing new, so they fetched no messages.
    assert_eq!(
        reads("get_messages"),
        3,
        "the burst was not one re-read of the message and one more: {names:?}"
    );
}
