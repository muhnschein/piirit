//! Which transcription may run now, which runs next. Pure: settings and power in, answer out.
//!
//! Self-contained: tests include it by `#[path]`.

/// `Transcriber.mode`: taps only.
pub(crate) const MODE_NEVER: u32 = 0;
/// `Transcriber.mode`: arrivals run while charging.
pub(crate) const MODE_CHARGING: u32 = 1;
/// `Transcriber.mode`: arrivals run unless battery saving.
pub(crate) const MODE_ALWAYS: u32 = 2;

/// Why a job is queued. Order = priority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    /// Tap, or `transcribe_chat`.
    Manual,
    /// Voice message arrived (fully downloaded).
    New,
    /// Older voice message without transcript (`older`).
    Backlog,
}

/// Everything deciding whether a job runs.
// One flag per input: no state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Conditions {
    /// Feature on.
    pub enabled: bool,
    /// Model on phone, checked.
    pub model_ready: bool,
    /// `MODE_*`.
    pub mode: u32,
    /// Older voice messages too.
    pub older: bool,
    /// Charger in.
    pub charging: bool,
    /// Battery saving on.
    pub power_saving: bool,
}

/// Job of `kind` may start now.
pub(crate) fn may_run(kind: Kind, conditions: &Conditions) -> bool {
    if !conditions.enabled || !conditions.model_ready {
        return false;
    }
    let plugged = conditions.charging && !conditions.power_saving;
    match kind {
        Kind::Manual => true,
        Kind::New => match conditions.mode {
            MODE_CHARGING => plugged,
            MODE_ALWAYS => !conditions.power_saving,
            _ => false,
        },
        Kind::Backlog => conditions.older && plugged,
    }
}

/// Job of `kind` stays queued under these settings; else dropped (would never run).
pub(crate) fn keeps(kind: Kind, conditions: &Conditions) -> bool {
    conditions.enabled
        && match kind {
            Kind::Manual => true,
            Kind::New => conditions.mode != MODE_NEVER,
            Kind::Backlog => conditions.older,
        }
}

/// Arriving voice messages queue.
pub(crate) fn queues_new(conditions: &Conditions) -> bool {
    keeps(Kind::New, conditions)
}

/// Older voice messages queue.
pub(crate) fn queues_backlog(conditions: &Conditions) -> bool {
    keeps(Kind::Backlog, conditions)
}

/// What ordering reads of a job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// Why queued.
    pub kind: Kind,
    /// Queue order: taps and arrivals first come, first served.
    pub seq: u64,
    /// Message time: backlog newest first.
    pub timestamp: i64,
}

/// Index of job to start now, if any: manual first, then arrivals, then backlog.
pub(crate) fn next(entries: &[Entry], conditions: &Conditions) -> Option<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| may_run(entry.kind, conditions))
        .min_by_key(|(_, entry)| {
            let newest_first = if entry.kind == Kind::Backlog {
                entry.timestamp.saturating_neg()
            } else {
                0
            };
            (entry.kind, newest_first, entry.seq)
        })
        .map(|(index, _)| index)
}
