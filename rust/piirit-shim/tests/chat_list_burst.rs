//! Event burst: one refresh in flight, rest wait for one more. Each arrival is three core
//! events; catching up hundreds asked core for whole list per event.

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r#"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatList { id: chats; account_id: 1; paused: startPaused }
        property bool startPaused: false
        // Ten arrivals as core sends them, faster than one answer.
        function burst() {
            for (var i = 0; i < 10; i++) {
                chats.handle_event(1, 'IncomingMsg', '{"chatId": 2, "msgId": ' + (100 + i) + '}')
                chats.handle_event(1, 'ChatlistItemChanged', '{"chatId": 2}')
                chats.handle_event(1, 'ChatlistChanged', '{}')
            }
        }
        function count() { return '' + chats.count }
        function changed(chatId) {
            chats.handle_event(1, 'ChatlistItemChanged', '{"chatId": ' + chatId + '}')
        }
        function reordered() { chats.handle_event(1, 'ChatlistChanged', '{}') }
        function reload() { chats.reload() }
        // Chat 2 changed, then search typed fast (two letters a turn): restart replaces
        // refresh in flight.
        function refilter() {
            changed(2)
            chats.query = "cha"
            chats.query = "chat"
        }
        function pause(on) { chats.paused = on }
    }
"#;

/// Listings asked at fake core.
fn listings(journal: &std::path::Path) -> usize {
    common::methods(journal)
        .iter()
        .filter(|name| *name == "get_chatlist_entries")
        .count()
}

/// Chats asked for row by row, per ask.
fn item_asks(journal: &std::path::Path) -> Vec<Vec<u64>> {
    common::calls(journal)
        .into_iter()
        .filter(|(name, _)| name == "get_chatlist_items_by_entries")
        .map(|(_, params)| {
            params
                .get(1)
                .and_then(Value::as_array)
                .map(|ids| ids.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default()
        })
        .collect()
}

/// Probe handles for plan steps. SAFETY: steps fire only inside `exec()`; engine and steps
/// outlive it.
#[derive(Clone, Copy)]
struct Probe {
    engine: *mut QmlEngine,
    steps: *mut Vec<(&'static str, String)>,
}

impl Probe {
    fn call(self, name: &str, args: &[QVariant]) -> String {
        // SAFETY: see struct.
        unsafe {
            QString::from_qvariant((*self.engine).invoke_method(name.into(), args))
                .map(|text| text.to_string())
                .unwrap_or_default()
        }
    }

    fn note(self, label: &'static str, value: &impl ToString) {
        // SAFETY: see struct.
        unsafe { (*self.steps).push((label, value.to_string())) }
    }

    fn loaded(self) -> bool {
        self.call("count", &[]) != "0"
    }
}

/// Paused from start: nothing loads, burst only noted; then shown, hidden, hidden mid-refresh.
fn plan_paused(plan: &mut common::Plan, probe: Probe, journal: &std::path::Path) {
    let at = journal.to_path_buf();
    plan.after(3, move || {
        probe.note("paused-listings", &listings(&at));
        probe.call("burst", &[]);
    });
    let at = journal.to_path_buf();
    plan.after(2, move || {
        probe.note("burst-listings", &listings(&at));
        probe.call("pause", &[false.into()]);
    });
    // Shown, then hidden again (cover put away): next burst waits too.
    let at = journal.to_path_buf();
    plan.until(1, 30, move |last| {
        if !probe.loaded() && !last {
            return false;
        }
        probe.note("resumed-listings", &listings(&at));
        probe.call("pause", &[true.into()]);
        probe.call("burst", &[]);
        true
    });
    let at = journal.to_path_buf();
    plan.after(3, move || {
        probe.note("hidden-listings", &listings(&at));
        probe.call("pause", &[false.into()]);
    });
    // Hidden while one is out: its answer kept, burst's waits for next look.
    let at = journal.to_path_buf();
    plan.after(3, move || {
        probe.note("shown-listings", &listings(&at));
        probe.call("burst", &[]);
        probe.call("pause", &[true.into()]);
    });
    let at = journal.to_path_buf();
    plan.after(3, move || {
        probe.note("landed-listings", &listings(&at));
        probe.call("pause", &[false.into()]);
    });
}

/// Row fetches fail once each: chat 2's, then whole reload's; then search replaces refresh.
fn plan_failing(
    plan: &mut common::Plan,
    probe: Probe,
    journal: &std::path::Path,
    fail: &std::path::Path,
) {
    let (at, fail_at) = (journal.to_path_buf(), fail.to_path_buf());
    plan.until(1, 30, move |last| {
        if !probe.loaded() && !last {
            return false;
        }
        probe.note("before", &item_asks(&at).len());
        std::fs::write(&fail_at, "get_chatlist_items_by_entries").expect("write fail file");
        probe.call("changed", &[2.into()]);
        true
    });
    plan.after(3, move || {
        probe.call("reordered", &[]);
    });
    let (at, fail_at) = (journal.to_path_buf(), fail.to_path_buf());
    plan.after(3, move || {
        probe.note("before-all", &item_asks(&at).len());
        std::fs::write(&fail_at, "get_chatlist_items_by_entries").expect("write fail file");
        probe.call("reload", &[]);
    });
    plan.after(3, move || {
        probe.call("reordered", &[]);
    });
    let at = journal.to_path_buf();
    plan.after(3, move || {
        probe.note("before-search", &item_asks(&at).len());
        probe.note("listings-before-search", &listings(&at));
        probe.note("rows", &probe.call("count", &[]));
        probe.call("refilter", &[]);
    });
}

fn run(name: &str, paused: bool) -> (Vec<(&'static str, String)>, Vec<Value>) {
    let temp = std::env::temp_dir().join(format!("piirit-burst-{name}-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    let fail = temp.join("fail");
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Burst lands well inside one answer.
        std::env::set_var("PIIRIT_FAKE_CHATLIST_DELAY_MS", "1500");
        std::env::set_var("PIIRIT_FAKE_FAIL_ONCE", &fail);
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let mut steps: Vec<(&'static str, String)> = Vec::new();
    let probe = Probe {
        engine: std::ptr::addr_of_mut!(engine),
        steps: std::ptr::addr_of_mut!(steps),
    };
    let core_ptr = std::ptr::addr_of!(core_box);
    let mut plan = common::Plan::default();
    // SAFETY: fires inside `exec()`; `core_box` outlives it.
    plan.until(1, 30, move |_| unsafe { common::ready(&*core_ptr) });
    plan.after(0, move || {
        // SAFETY: see `Probe`.
        unsafe {
            (*probe.engine).load_data(QByteArray::from(PROBE_QML.replace(
                "property bool startPaused: false",
                &format!("property bool startPaused: {paused}"),
            )));
        }
    });
    if paused {
        plan_paused(&mut plan, probe, &journal);
    } else if name == "failing" {
        plan_failing(&mut plan, probe, &journal, &fail);
    } else {
        let at = journal.clone();
        plan.until(1, 30, move |last| {
            if !probe.loaded() && !last {
                return false;
            }
            probe.note("before", &listings(&at));
            probe.call("burst", &[]);
            true
        });
    }
    // Two answers' time: settled, nothing more asked.
    let at = journal.clone();
    plan.after(5, move || {
        probe.note("after", &listings(&at));
        probe.note("count", &probe.call("count", &[]));
        // SAFETY: see `Probe`.
        unsafe { (*probe.engine).quit() };
    });
    plan.run();
    engine.exec();
    (steps, common::records(&journal))
}

fn value(steps: &[(&str, String)], label: &str) -> usize {
    steps
        .iter()
        .find(|(name, _)| *name == label)
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(usize::MAX)
}

#[test]
fn a_burst_of_arrivals_asks_for_the_list_twice_not_thirty_times() {
    let (steps, _) = run("live", false);
    let context = format!("steps: {steps:?}");
    let before = value(&steps, "before");
    let after = value(&steps, "after");
    assert!(
        (1..usize::MAX).contains(&before),
        "the list never loaded. {context}"
    );
    // One for first event, one for the 29 that came while it was out.
    assert_eq!(
        after.saturating_sub(before),
        2,
        "thirty events while one refresh was out did not fold into one more. {context}"
    );
}

#[test]
fn a_paused_list_reads_nothing_until_resumed_then_once_each_time() {
    let (steps, _) = run("paused", true);
    let context = format!("steps: {steps:?}");
    assert_eq!(
        value(&steps, "paused-listings"),
        0,
        "a paused list loaded. {context}"
    );
    assert_eq!(
        value(&steps, "burst-listings"),
        0,
        "events reached the core while paused. {context}"
    );
    assert_eq!(
        value(&steps, "resumed-listings"),
        1,
        "resuming did not load the list once. {context}"
    );
    assert_eq!(
        value(&steps, "hidden-listings"),
        1,
        "events reached the core while paused again. {context}"
    );
    assert_eq!(
        value(&steps, "shown-listings"),
        2,
        "resuming again did not catch up once. {context}"
    );
    assert_eq!(
        value(&steps, "landed-listings"),
        3,
        "answer landing while paused asked again. {context}"
    );
    assert_eq!(
        value(&steps, "after"),
        4,
        "resuming did not ask what came meanwhile. {context}"
    );
    assert!(
        value(&steps, "count") > 0,
        "resuming showed no rows. {context}"
    );
}

#[test]
fn a_refresh_that_fails_or_is_replaced_leaves_its_chats_owed() {
    let temp = std::env::temp_dir().join(format!("piirit-burst-failing-{}", std::process::id()));
    let (steps, _) = run("failing", false);
    let asks = item_asks(&temp.join("journal.jsonl"));
    let context = format!("steps: {steps:?}, row asks: {asks:?}");
    let before = value(&steps, "before");
    assert!(before <= asks.len(), "the list never loaded. {context}");
    let all_from = value(&steps, "before-all");
    let for_two = asks[before..all_from.min(asks.len())]
        .iter()
        .filter(|ids| ids.contains(&2))
        .count();
    // Failed ask, then refresh after it asks chat 2 again though only order changed.
    assert_eq!(
        for_two, 2,
        "chat 2's failed fetch was not asked again. {context}"
    );
    let rows = value(&steps, "rows");
    let search_from = value(&steps, "before-search");
    let whole: Vec<usize> = asks[all_from..search_from.min(asks.len())]
        .iter()
        .map(Vec::len)
        .collect();
    assert_eq!(
        whole,
        vec![rows, rows],
        "a failed reload was not followed by every row again. {context}"
    );
    // Replaced refresh's answer dropped: replacement asks chat 2 too, and one listing per turn.
    let two_after = asks
        .iter()
        .skip(search_from)
        .filter(|ids| ids.contains(&2))
        .count();
    assert_eq!(
        two_after, 2,
        "the search dropped what the refresh it replaced owed. {context}"
    );
    assert_eq!(
        value(&steps, "after") - value(&steps, "listings-before-search"),
        2,
        "two letters in one turn listed more than once after the replaced refresh. {context}"
    );
}
