//! When transcription runs: every mode × charger × battery saving × job kind, and order.

#[allow(dead_code)]
#[path = "../src/transcripts/schedule.rs"]
mod schedule;

use schedule::{
    keeps, may_run, next, queues_backlog, queues_new, Conditions, Entry, Kind, MODE_ALWAYS,
    MODE_CHARGING, MODE_NEVER,
};

const KINDS: [Kind; 3] = [Kind::Manual, Kind::New, Kind::Backlog];

/// The rule as the settings page states it.
fn expected(kind: Kind, c: &Conditions) -> bool {
    if !c.enabled || !c.model_ready {
        return false;
    }
    match kind {
        Kind::Manual => true,
        Kind::New if c.mode == MODE_CHARGING => c.charging && !c.power_saving,
        Kind::New if c.mode == MODE_ALWAYS => !c.power_saving,
        Kind::New => false,
        Kind::Backlog => c.older && c.charging && !c.power_saving,
    }
}

/// Every combination of inputs.
fn all() -> Vec<Conditions> {
    let mut out = Vec::new();
    for bits in 0..32_u32 {
        for mode in [MODE_NEVER, MODE_CHARGING, MODE_ALWAYS] {
            out.push(Conditions {
                enabled: bits & 1 != 0,
                model_ready: bits & 2 != 0,
                older: bits & 4 != 0,
                charging: bits & 8 != 0,
                power_saving: bits & 16 != 0,
                mode,
            });
        }
    }
    out
}

#[test]
fn every_mode_charger_and_battery_saving_case_runs_as_the_settings_say() {
    for c in all() {
        for kind in KINDS {
            assert_eq!(
                may_run(kind, &c),
                expected(kind, &c),
                "{kind:?} under {c:?}"
            );
        }
    }
}

#[test]
fn taps_ignore_charger_and_battery_saving_but_not_the_model_or_the_switch() {
    let on = Conditions {
        enabled: true,
        model_ready: true,
        mode: MODE_NEVER,
        older: false,
        charging: false,
        power_saving: true,
    };
    assert!(may_run(Kind::Manual, &on));
    assert!(!may_run(
        Kind::Manual,
        &Conditions {
            model_ready: false,
            ..on
        }
    ));
    assert!(!may_run(
        Kind::Manual,
        &Conditions {
            enabled: false,
            ..on
        }
    ));
}

#[test]
fn spelled_out_cases() {
    let base = Conditions {
        enabled: true,
        model_ready: true,
        mode: MODE_CHARGING,
        older: true,
        charging: true,
        power_saving: false,
    };
    // While charging: plugged runs, unplugged waits, battery saving pauses.
    assert!(may_run(Kind::New, &base));
    assert!(!may_run(
        Kind::New,
        &Conditions {
            charging: false,
            ..base
        }
    ));
    assert!(!may_run(
        Kind::New,
        &Conditions {
            power_saving: true,
            ..base
        }
    ));
    // Always: unplugged runs; battery saving pauses.
    let always = Conditions {
        mode: MODE_ALWAYS,
        charging: false,
        ..base
    };
    assert!(may_run(Kind::New, &always));
    assert!(!may_run(
        Kind::New,
        &Conditions {
            power_saving: true,
            ..always
        }
    ));
    // Backlog needs the charger even in Always.
    assert!(!may_run(Kind::Backlog, &always));
    assert!(may_run(
        Kind::Backlog,
        &Conditions {
            mode: MODE_NEVER,
            ..base
        }
    ));
    assert!(!may_run(
        Kind::Backlog,
        &Conditions {
            older: false,
            ..base
        }
    ));
    // Never: arrivals don't run even plugged in.
    assert!(!may_run(
        Kind::New,
        &Conditions {
            mode: MODE_NEVER,
            ..base
        }
    ));
}

#[test]
fn jobs_settings_can_never_run_are_dropped_not_kept_waiting() {
    for c in all() {
        assert_eq!(keeps(Kind::Manual, &c), c.enabled, "{c:?}");
        assert_eq!(
            keeps(Kind::New, &c),
            c.enabled && c.mode != MODE_NEVER,
            "{c:?}"
        );
        assert_eq!(keeps(Kind::Backlog, &c), c.enabled && c.older, "{c:?}");
        assert_eq!(queues_new(&c), keeps(Kind::New, &c));
        assert_eq!(queues_backlog(&c), keeps(Kind::Backlog, &c));
        // Kept but not runnable only for power or model: waits. Runnable is always kept.
        for kind in KINDS {
            if may_run(kind, &c) {
                assert!(keeps(kind, &c), "{kind:?} runs but would be dropped: {c:?}");
            }
        }
    }
}

fn entry(kind: Kind, seq: u64, timestamp: i64) -> Entry {
    Entry {
        kind,
        seq,
        timestamp,
    }
}

#[test]
fn taps_first_then_arrivals_in_order_then_newest_backlog() {
    let plugged = Conditions {
        enabled: true,
        model_ready: true,
        mode: MODE_CHARGING,
        older: true,
        charging: true,
        power_saving: false,
    };
    let queue = [
        entry(Kind::Backlog, 1, 100),
        entry(Kind::New, 5, 900),
        entry(Kind::Backlog, 2, 300),
        entry(Kind::Manual, 9, 50),
        entry(Kind::New, 4, 800),
        entry(Kind::Manual, 7, 60),
    ];
    // Order taken one by one.
    let mut left: Vec<Entry> = queue.to_vec();
    let mut order = Vec::new();
    while let Some(index) = next(&left, &plugged) {
        order.push(left.remove(index).seq);
    }
    assert_eq!(order, vec![7, 9, 4, 5, 2, 1]);

    // Unplugged, While charging: only taps.
    let unplugged = Conditions {
        charging: false,
        ..plugged
    };
    let mut left: Vec<Entry> = queue.to_vec();
    let mut order = Vec::new();
    while let Some(index) = next(&left, &unplugged) {
        order.push(left.remove(index).seq);
    }
    assert_eq!(order, vec![7, 9]);
    assert_eq!(left.len(), 4, "arrivals and backlog wait for the charger");

    // Model missing: nothing.
    assert_eq!(
        next(
            &queue,
            &Conditions {
                model_ready: false,
                ..plugged
            }
        ),
        None
    );
    assert_eq!(next(&[], &plugged), None);
}
