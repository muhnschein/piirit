//! Make profile (chatmail relay or own mailbox, `transport_call`) or take one over (device or
//! backup, `restore`): one attempt at a time, with deadline.
//!
//! Attempt: account, name, core transport call (asks relay for address or tries mailbox, stores,
//! starts IO); silent relay holds it for minutes of connection attempts. Past [`DEADLINE`]
//! (under call's one-minute client limit): `profile_timed_out`, core process stopped. Page has
//! Cancel throughout, "try another relay" from 4 s.
//!
//! Core runs one process per account: given-up attempt still holds it, account unconfigured
//! till it ends; reusing it was refused ("There is already another ongoing process running").
//! `busy` tracks held accounts; retry takes a fresh one. Late success after cancel: removed,
//! unsignalled; relay added to existing profile kept (page's remove undoes it).

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use deltachat_jsonrpc::RpcClient;

use crate::json;
use crate::runtime::CoreRuntime;

/// Relay's whole budget: address plus two logins on slow link; down relay reported while reader
/// watches.
pub const DEADLINE: Duration = Duration::from_secs(30);

/// Where profile is reached: core-picked relays, `dcaccount:`, or own mailbox.
pub(crate) enum Transport {
    /// `init_transports`: first answering core relay; since 2.61 core adds more in background
    /// (only path that does). Named relay: that one only; extras enabled for this call alone.
    Automatic,
    /// `add_transport_from_qr`: relay mints address.
    Qr(String),
    /// `add_or_update_transport`: fields `EnteredLoginParam` cannot autoconfigure.
    Mailbox {
        /// Address.
        addr: String,
        /// Password.
        password: String,
    },
}

/// Where taken-over profile comes from.
pub(crate) enum Source {
    /// `get_backup`: code from other device, then LAN transfer.
    Device(String),
    /// `import_backup`: backup file.
    File(String),
}

/// How taking over ended.
pub(crate) enum Taken {
    /// Profile on this device, as `account_id`.
    Done(u32),
    /// App reason, page translates: `not-a-backup`, `too-new`, `stalled`, `already-here`.
    Refused(&'static str),
    /// Core refused, its words.
    Failed(String),
}

/// Cap on transfer, not a deadline: ends one whose far end vanished.
pub const TRANSFER_DEADLINE: Duration = Duration::from_secs(30 * 60);

/// How attempt ended.
pub(crate) enum Outcome {
    /// Made; `account_id` has working transport.
    Created(u32),
    /// Core refused.
    Failed(String),
    /// Relay silent this many seconds.
    TimedOut(u32),
}

/// Attempts, shared with tasks. Atomic counters; set lock never held across await.
#[derive(Default)]
pub(crate) struct Attempts {
    /// Attempt reader waits on: latest, or 0 once cancelled.
    current: AtomicU64,
    /// Attempts begun; next id.
    begun: AtomicU64,
    /// Accounts with transport call running.
    busy: Mutex<BTreeSet<u32>>,
}

impl Attempts {
    /// Start attempt; it counts.
    pub(crate) fn begin(self: &Arc<Self>, deadline: Duration) -> Attempt {
        // Wrapping, as `fetch_add`.
        let id = self.begun.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        self.current.store(id, Ordering::SeqCst);
        Attempt {
            id,
            shared: self.clone(),
            deadline,
            wanted: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Reader gave up: nothing counts. Returns held accounts.
    pub(crate) fn cancel(&self) -> Vec<u32> {
        self.current.store(0, Ordering::SeqCst);
        self.busy().iter().copied().collect()
    }

    /// Whether `attempt` is still awaited.
    pub(crate) fn is_current(&self, attempt: u64) -> bool {
        self.current.load(Ordering::SeqCst) == attempt
    }

    fn busy(&self) -> std::sync::MutexGuard<'_, BTreeSet<u32>> {
        self.busy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Account is nobody's now.
    fn release(&self, account_id: u32) {
        self.busy().remove(&account_id);
    }
}

/// One attempt at making a profile.
pub(crate) struct Attempt {
    id: u64,
    shared: Arc<Attempts>,
    deadline: Duration,
    /// Cleared at deadline; result then unwanted.
    wanted: Arc<AtomicBool>,
}

impl Attempt {
    /// Attempt id.
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// Hold `account_id` before task spawns, for same-turn cancel.
    pub(crate) fn hold(&self, account_id: u32) {
        self.shared.busy().insert(account_id);
    }

    /// Do the work: account, name, transport.
    pub(crate) async fn run(
        &self,
        runtime: &CoreRuntime,
        rpc: Arc<RpcClient>,
        display_name: String,
        transport: Transport,
    ) -> Outcome {
        let account_id = match self.pick_account(&rpc).await {
            Ok(account_id) => account_id,
            Err(err) => return Outcome::Failed(err),
        };
        // Name before transport call: set when core announces account. Cancelled: relay unasked.
        let prepared = set_display_name(&rpc, account_id, display_name).await;
        if prepared.is_err() || !self.shared.is_current(self.id) {
            self.shared.release(account_id);
            return prepared.map_or_else(Outcome::Failed, |()| {
                Outcome::Failed("cancelled".to_string())
            });
        }

        self.transport_within_deadline(runtime, rpc, account_id, transport, Made::Here)
            .await
    }

    /// Add relay to existing profile: transport call alone, same deadline, held for cancel.
    pub(crate) async fn add_transport(
        &self,
        runtime: &CoreRuntime,
        rpc: Arc<RpcClient>,
        account_id: u32,
        transport: Transport,
    ) -> Outcome {
        self.hold(account_id);
        self.transport_within_deadline(runtime, rpc, account_id, transport, Made::Before)
            .await
    }

    /// Transport call, given up at deadline. Own task finishes anyway; account held till core
    /// answers.
    async fn transport_within_deadline(
        &self,
        runtime: &CoreRuntime,
        rpc: Arc<RpcClient>,
        account_id: u32,
        transport: Transport,
        made: Made,
    ) -> Outcome {
        let call = runtime.spawn(transport_call(
            rpc.clone(),
            self.shared.clone(),
            self.wanted.clone(),
            account_id,
            transport,
            made,
        ));
        match tokio::time::timeout(self.deadline, call).await {
            Ok(Ok(Ok(()))) => Outcome::Created(account_id),
            Ok(Ok(Err(err))) => Outcome::Failed(err),
            Ok(Err(err)) => Outcome::Failed(err.to_string()),
            Err(_) => {
                self.wanted.store(false, Ordering::SeqCst);
                // Abandoned: `wanted` stops it landing.
                rpc.call::<_, ()>("stop_ongoing_process", (account_id,))
                    .await
                    .ok();
                Outcome::TimedOut(u32::try_from(self.deadline.as_secs()).unwrap_or(u32::MAX))
            }
        }
    }

    /// Take profile over: account, code read, core import (`ImexProgress`). Failed import or
    /// duplicate: account removed, not released (half-written useless, unlike refused signup's).
    pub(crate) async fn restore(
        &self,
        runtime: &CoreRuntime,
        rpc: Arc<RpcClient>,
        source: Source,
    ) -> Taken {
        let account_id = match self.pick_account(&rpc).await {
            Ok(account_id) => account_id,
            Err(err) => return Taken::Failed(err),
        };
        if let Source::Device(qr) = &source {
            if let Err(reason) = from_a_device(&rpc, account_id, qr).await {
                self.shared.release(account_id);
                return Taken::Refused(reason);
            }
        }
        // Own task, like transport call.
        let call = runtime.spawn(import_call(
            rpc.clone(),
            self.shared.clone(),
            self.wanted.clone(),
            account_id,
            source,
        ));
        match tokio::time::timeout(self.deadline, call).await {
            Ok(Ok(Ok(()))) => {
                // Second copy of mailbox fetches twice (counts, notifications): undo import;
                // existing copy untouched.
                if already_here(&rpc, account_id).await {
                    discard(&rpc, account_id).await;
                    return Taken::Refused("already-here");
                }
                Taken::Done(account_id)
            }
            Ok(Ok(Err(err))) => {
                discard(&rpc, account_id).await;
                Taken::Failed(err)
            }
            Ok(Err(err)) => {
                discard(&rpc, account_id).await;
                Taken::Failed(err.to_string())
            }
            Err(_) => {
                self.wanted.store(false, Ordering::SeqCst);
                // Best effort, as for signup.
                rpc.call::<_, ()>("stop_ongoing_process", (account_id,))
                    .await
                    .ok();
                Taken::Refused("stalled")
            }
        }
    }

    /// Unconfigured unheld account, else fresh. Reuse stops stranding one per retry; held check
    /// avoids one still configuring.
    async fn pick_account(&self, rpc: &RpcClient) -> Result<u32, String> {
        let accounts: Vec<serde_json::Value> = rpc
            .call_unit("get_all_accounts")
            .await
            .map_err(|err| err.to_string())?;
        {
            let mut busy = self.shared.busy();
            let free = accounts
                .iter()
                .filter(|account| json::str_at(account, "kind") == "Unconfigured")
                .filter_map(|account| json::u32_opt(account, "id"))
                .find(|id| !busy.contains(id));
            if let Some(account_id) = free {
                busy.insert(account_id);
                return Ok(account_id);
            }
        }
        let account_id = rpc
            .call_unit::<u32>("add_account")
            .await
            .map_err(|err| err.to_string())?;
        self.shared.busy().insert(account_id);
        Ok(account_id)
    }
}

/// Whether transport call's account was made for it.
#[derive(Clone, Copy)]
enum Made {
    /// Being made: removed if nobody wants it.
    Here,
    /// Relay added: profile is reader's; kept.
    Before,
}

/// Transport call, then release. Success nobody awaits is removed if account made for it.
async fn transport_call(
    rpc: Arc<RpcClient>,
    shared: Arc<Attempts>,
    wanted: Arc<AtomicBool>,
    account_id: u32,
    transport: Transport,
    made: Made,
) -> Result<(), String> {
    let result = match transport {
        Transport::Automatic => {
            rpc.call::<_, ()>("init_transports", (account_id, Option::<String>::None))
                .await
        }
        Transport::Qr(qr) => {
            rpc.call::<_, ()>("add_transport_from_qr", (account_id, qr))
                .await
        }
        Transport::Mailbox { addr, password } => {
            let param = serde_json::json!({ "addr": addr, "password": password });
            rpc.call::<_, ()>("add_or_update_transport", (account_id, param))
                .await
        }
    }
    .map_err(|err| err.to_string());
    shared.release(account_id);
    if result.is_ok() && !wanted.load(Ordering::SeqCst) && matches!(made, Made::Here) {
        discard(&rpc, account_id).await;
    }
    result
}

/// Import call, then release. Taken over after give-up: removed.
async fn import_call(
    rpc: Arc<RpcClient>,
    shared: Arc<Attempts>,
    wanted: Arc<AtomicBool>,
    account_id: u32,
    source: Source,
) -> Result<(), String> {
    let result = match source {
        Source::Device(qr) => rpc.call::<_, ()>("get_backup", (account_id, qr)).await,
        Source::File(path) => {
            rpc.call::<_, ()>("import_backup", (account_id, path, Option::<String>::None))
                .await
        }
    }
    .map_err(|err| err.to_string());
    shared.release(account_id);
    if result.is_ok() && !wanted.load(Ordering::SeqCst) {
        discard(&rpc, account_id).await;
    }
    result
}

/// Another configured account reaches an address of profile just taken over?
///
/// By every transport address (`list_transports`): account list lacks address since 2.61;
/// `configured_addr` can differ between copies. Asked of core, not remembered: earlier runs'
/// copies count. Unreadable account passes (wrong refusal worse). One call per configured
/// profile till match; once per takeover.
async fn already_here(rpc: &RpcClient, account_id: u32) -> bool {
    let Ok(accounts) = rpc
        .call_unit::<Vec<serde_json::Value>>("get_all_accounts")
        .await
    else {
        return false;
    };
    let arrived = transport_addrs(rpc, account_id).await;
    if arrived.is_empty() {
        return false;
    }
    for account in &accounts {
        let Some(other) = json::u32_opt(account, "id") else {
            continue;
        };
        if other == account_id || json::str_at(account, "kind") != "Configured" {
            continue;
        }
        // Core lowercases; other clients' backups may not.
        let theirs = transport_addrs(rpc, other).await;
        if theirs
            .iter()
            .any(|addr| arrived.iter().any(|mine| mine.eq_ignore_ascii_case(addr)))
        {
            return true;
        }
    }
    false
}

/// Addresses of transports; empty if core cannot say.
async fn transport_addrs(rpc: &RpcClient, account_id: u32) -> Vec<String> {
    rpc.call::<_, Vec<serde_json::Value>>("list_transports", (account_id,))
        .await
        .map(|transports| {
            transports
                .iter()
                .map(|transport| json::str_at(transport, "addr"))
                .filter(|addr| !addr.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Code from another device's offer? Checked first: core's non-code error talks protocols;
/// reader holding camera needs "not a backup".
async fn from_a_device(rpc: &RpcClient, account_id: u32, qr: &str) -> Result<(), &'static str> {
    let checked: serde_json::Value =
        rpc.call("check_qr", (account_id, qr))
            .await
            .map_err(|err| {
                // Reader sees "not-a-backup"; core's reason is logged.
                eprintln!("piirit: checking a code for a profile: {err}");
                "not-a-backup"
            })?;
    match json::str_at(&checked, "kind") {
        "backup2" => Ok(()),
        // Newer Delta Chat than this core.
        "backupTooNew" => Err("too-new"),
        _ => Err("not-a-backup"),
    }
}

/// Remove profile made after reader gave up.
pub(crate) async fn discard(rpc: &RpcClient, account_id: u32) {
    // Failure leaves empty profile in switcher; logged only.
    if let Err(err) = rpc.call::<_, ()>("remove_account", (account_id,)).await {
        eprintln!("piirit: removing an abandoned profile: {err}");
    }
}

/// Set display name.
async fn set_display_name(
    rpc: &RpcClient,
    account_id: u32,
    display_name: String,
) -> Result<(), String> {
    rpc.call::<_, ()>(
        "set_config",
        (account_id, "displayname", Some(display_name)),
    )
    .await
    .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_attempt_is_the_one_that_counts() {
        let attempts = Arc::new(Attempts::default());
        let first = attempts.begin(DEADLINE);
        assert!(attempts.is_current(first.id()));
        let second = attempts.begin(DEADLINE);
        assert!(!attempts.is_current(first.id()));
        assert!(attempts.is_current(second.id()));
        assert_ne!(first.id(), second.id());
    }

    #[test]
    fn a_cancel_leaves_no_attempt_current_and_names_what_is_held() {
        let attempts = Arc::new(Attempts::default());
        let attempt = attempts.begin(DEADLINE);
        attempts.busy().insert(7);
        attempts.busy().insert(3);
        assert_eq!(attempts.cancel(), vec![3, 7]);
        assert!(!attempts.is_current(attempt.id()));
        // Cancel stops waiting, not holding.
        assert_eq!(attempts.busy().len(), 2);
        attempts.release(7);
        assert_eq!(attempts.busy().iter().copied().collect::<Vec<_>>(), vec![3]);
    }

    #[test]
    fn an_account_held_before_its_task_runs_is_named_by_a_cancel() {
        let attempts = Arc::new(Attempts::default());
        let attempt = attempts.begin(DEADLINE);
        attempt.hold(1);
        // No task has run yet: cancel still finds account.
        assert_eq!(attempts.cancel(), vec![1]);
    }
}
