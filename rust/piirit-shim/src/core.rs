use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use deltachat_jsonrpc::{spawn_event_loop, CoreEvent, RpcClient};
use qmetaobject::*;

use crate::json;
use crate::models::{AccountItem, AccountListModel};
use crate::runtime::CoreRuntime;
use crate::signup::{self, Attempts, Outcome, Source, Taken, Transport};

/// Live server connection, shared by per-chat models. One per process, unenforced: `start` guard
/// is per object; second instance would spawn second server and take this over.
static CONNECTION: Mutex<Option<(Arc<RpcClient>, CoreRuntime)>> = Mutex::new(None);

/// Transport and runtime, once [`DeltaChatCore::start`] done.
pub(crate) fn connection() -> Option<(Arc<RpcClient>, CoreRuntime)> {
    CONNECTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn set_connection(value: Option<(Arc<RpcClient>, CoreRuntime)>) {
    *CONNECTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = value;
}

/// Default chatmail server, `dcaccount:` payload. See `docs/PROJECT.md`.
pub const DEFAULT_PROVIDER_QR: &str = "dcaccount:nine.testrun.org";

/// RPM install path: not on `PATH`.
pub const BUNDLED_SERVER: &str = "/usr/libexec/harbour-piirit/deltachat-rpc-server";

/// Kinds reaching `core_event`; rest (log, unused: most of bulk syncs) dropped early.
///
/// Union of kinds matched by any `handle_event` here and named by any `onCore_event` in `qml/`.
/// **New reader kind: add here**, or it never arrives. `tests/event_kinds.rs`: sides match;
/// `tests/event_fanout.rs`: gate applied.
pub const HANDLED_EVENT_KINDS: &[&str] = &[
    "CallEnded",
    "ChatDeleted",
    "ChatEphemeralTimerModified",
    "ChatModified",
    "ChatlistChanged",
    "ChatlistItemChanged",
    "ConnectivityChanged",
    "ContactsChanged",
    "EventChannelOverflow",
    "ImexProgress",
    "IncomingCall",
    "IncomingCallAccepted",
    "IncomingMsg",
    "MsgDeleted",
    "MsgDelivered",
    "MsgFailed",
    "MsgRead",
    "MsgsChanged",
    "MsgsNoticed",
    "OutgoingCallAccepted",
    "ReactionsChanged",
    "TransportsModified",
    "WebxdcInstanceDeleted",
    "WebxdcStatusUpdate",
];

/// Kinds that can move unread count (overflow may hide any); `relay` rereads it on each.
const UNREAD_EVENT_KINDS: &[&str] = &[
    "ChatDeleted",
    "ChatModified",
    "ChatlistChanged",
    "ChatlistItemChanged",
    "EventChannelOverflow",
    "IncomingMsg",
    "IncomingMsgBunch",
    "MsgsChanged",
    "MsgsNoticed",
];

/// Kinds `relay` turns into typed signals. Keep in step by hand.
const SIGNALLED_EVENT_KINDS: &[&str] = &["ConfigureProgress", "Error", "ImexProgress"];

/// Cross to Qt thread only if `core_event` or `relay` reads it.
fn is_relayed(kind: &str) -> bool {
    HANDLED_EVENT_KINDS.contains(&kind)
        || UNREAD_EVENT_KINDS.contains(&kind)
        || SIGNALLED_EVENT_KINDS.contains(&kind)
}

/// Max events per queued call; sync floods split so Qt draws between.
const EVENT_BATCH_LIMIT: usize = 256;

/// Exit wait for server.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Exit wait for live call to end (`call_host::end_all`) before server stop.
const CALL_END_TIMEOUT: Duration = Duration::from_secs(3);

/// Server binary: `--rpc-server <path>` / `--rpc-server=<path>`, else `PIIRIT_RPC_SERVER`, else
/// [`BUNDLED_SERVER`]. Never `PATH`: server gets mail password, keys. Flag, not `argv[1]`: Sailfish
/// invoker passes own args.
#[must_use]
pub fn server_path<I>(args: I, env: Option<String>) -> String
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--rpc-server" {
            if let Some(path) = args.next() {
                return path;
            }
        } else if let Some(path) = arg.strip_prefix("--rpc-server=") {
            return path.to_string();
        }
    }
    env.filter(|path| !path.is_empty())
        .unwrap_or_else(|| BUNDLED_SERVER.to_string())
}

/// Server pid, `None` before start or after exit. For `PIIRIT_MEMORY_LOG`.
#[must_use]
pub fn server_pid() -> Option<u32> {
    CONNECTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .and_then(|(rpc, _)| rpc.pid())
}

/// Stop server after Qt loop returns, bounded; else child dies only with last `RpcClient` (statics
/// never drop). Ends live call first, telling peer: only here is window (call host) up and server
/// not yet gone.
pub fn shutdown() {
    let Some((rpc, runtime)) = CONNECTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
    else {
        return;
    };
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    runtime.spawn(async move {
        // Call not ended in time: left to server exit.
        if tokio::time::timeout(CALL_END_TIMEOUT, crate::call_host::end_all())
            .await
            .is_err()
        {
            eprintln!("piirit: a call did not end in time; stopping the core anyway");
        }
        if let Err(err) = rpc.shutdown().await {
            eprintln!("piirit: stopping the core: {err}");
        }
        // Receiver gone only if wait gave up.
        done_tx.send(()).ok();
    });
    // Timeout quits without goodbye.
    done_rx
        .recv_timeout(CALL_END_TIMEOUT.saturating_add(SHUTDOWN_TIMEOUT))
        .ok();
}

/// Last server stderr lines, if any: user's only crash clue.
fn last_words(tail: &[String]) -> Option<String> {
    const KEEP: usize = 5;
    if tail.is_empty() {
        return None;
    }
    let start = tail.len().saturating_sub(KEEP);
    Some(format!(
        "the core's last output was: {}",
        tail.get(start..).unwrap_or(tail).join(" | ")
    ))
}

/// First restart wait, doubling to ceiling (stops spin). Short: phone reclaims memory, respawn
/// works.
const RESTART_DELAY_MIN: Duration = Duration::from_secs(1);
const RESTART_DELAY_MAX: Duration = Duration::from_secs(30);

/// Uptime counting as healthy; next failure backs off from minimum.
const HEALTHY_UPTIME: Duration = Duration::from_secs(60);

/// Restarts without healthy run before giving up (~3 min): outlasts transients; stops dead binary
/// keeping phone awake.
const RESTART_LIMIT: u32 = 12;

/// Wait before restart `attempt`.
fn restart_delay(attempt: u32) -> Duration {
    RESTART_DELAY_MIN
        .saturating_mul(1_u32.checked_shl(attempt).unwrap_or(u32::MAX))
        .min(RESTART_DELAY_MAX)
}

/// Owns spawned `deltachat-rpc-server`; slice of ~100 JSON-RPC methods. Fire-and-forget; results
/// via [`qmetaobject::queued_callback`] on Qt thread.
#[derive(QObject, Default)]
// Independent flags, not one state machine.
#[allow(clippy::struct_excessive_bools)]
pub struct DeltaChatCore {
    base: qt_base_class!(trait QObject),

    /// "idle", "starting", "ready", "stopped" (server died), or `"error: ..."`.
    pub status: qt_property!(QString; NOTIFY status_changed),
    /// `status` changed.
    pub status_changed: qt_signal!(),

    /// `get_system_info` JSON. Empty until [`DeltaChatCore::check_health`] runs.
    pub system_info: qt_property!(QString; NOTIFY system_info_changed),
    /// `system_info` refreshed.
    pub system_info_changed: qt_signal!(),

    /// Raw core event, whole as `payload_json`; untyped so QML reads any kind.
    pub core_event: qt_signal!(context_id: u32, kind: QString, payload_json: QString),

    /// New unconfigured account created.
    pub account_added: qt_signal!(account_id: u32),
    /// Account-scoped call failed.
    pub account_error: qt_signal!(message: QString),

    /// Spawn server, drain events. No-op if started.
    pub start: qt_method!(fn(&mut self, rpc_server_path: QString)),

    /// `get_system_info` round trip.
    pub check_health: qt_method!(fn(&mut self)),

    /// Create unconfigured account.
    pub add_account: qt_method!(fn(&mut self)),
    /// Delete profile and contents from device; no undo. `remove_account`: pinned core lacks
    /// `delete_account`.
    pub remove_account: qt_method!(fn(&mut self, account_id: u32)),

    /// All accounts known to core.
    pub account_list: qt_property!(RefCell<AccountListModel>; CONST),

    /// Core `download_limit`: attachments over this many bytes fetched on request; 0: all. Future
    /// messages only. Reader's, all accounts: written on set and on list reads to accounts lacking
    /// it (later profiles); same for settings below.
    pub download_limit: qt_property!(u32; WRITE set_download_limit NOTIFY download_limit_changed),
    /// `download_limit` changed.
    pub download_limit_changed: qt_signal!(),

    /// Core `media_quality`: 0 balanced, 1 smaller; read when recoding outgoing picture.
    pub media_quality: qt_property!(u32; WRITE set_media_quality NOTIFY media_quality_changed),
    /// `media_quality` changed.
    pub media_quality_changed: qt_signal!(),

    /// Core `who_can_call_me`: 0 all, 1 contacts, 2 nobody ring. Unrung call kept, answerable
    /// from chat.
    pub who_can_call_me: qt_property!(u32; WRITE set_who_can_call_me NOTIFY who_can_call_me_changed),
    /// `who_can_call_me` changed.
    pub who_can_call_me_changed: qt_signal!(),

    /// Core `delete_device_after`: delete from device after this many seconds; 0 keeps. Overrides
    /// chat timers; spares "Saved messages".
    pub delete_device_after: qt_property!(u32; WRITE set_delete_device_after NOTIFY delete_device_after_changed),
    /// `delete_device_after` changed.
    pub delete_device_after_changed: qt_signal!(),
    /// Count what `seconds` period would delete now, all accounts (shown before confirm). Answers
    /// `auto_deletion_estimated` or `core_error`.
    pub estimate_auto_deletion: qt_method!(fn(&mut self, seconds: u32)),
    /// Answer to `estimate_auto_deletion`; `seconds` tells stale ones apart.
    pub auto_deletion_estimated: qt_signal!(seconds: u32, count: u32),

    /// Repopulate `account_list`; QML resumes or onboards on `accounts_refreshed`.
    pub refresh_accounts: qt_method!(fn(&mut self)),
    /// `account_list` repopulated. `configured_count`: usable. `resume_account_id`: see
    /// `resume_account`.
    pub accounts_refreshed: qt_signal!(configured_count: u32, resume_account_id: u32),
    /// `account_id` configured, per last list read? Window asks per held id on refresh, to leave
    /// deleted profiles.
    pub is_configured_account: qt_method!(fn(&self, account_id: u32) -> bool),

    /// Tell core shown profile, for next start. Chat list calls per opened profile.
    pub select_account: qt_method!(fn(&mut self, account_id: u32)),

    /// Resume IO, one configured account.
    pub start_account_io: qt_method!(fn(&mut self, account_id: u32)),
    /// Resume IO, all configured accounts. All receive, even off-screen.
    pub start_all_account_io: qt_method!(fn(&mut self)),
    /// Result of resume. `account_id` 0: all.
    pub io_started: qt_signal!(account_id: u32, success: bool, error: QString),

    /// Stop IO, all accounts, only while phone reports no network
    /// (`qml/components/NetworkWatch.qml`): dead-network IO retries forever, each try waking radio;
    /// nothing to lose. Never on background: backgrounded app is only way messages arrive. Forgets
    /// requested IO, so offline restart stays stopped; restarter asks afresh.
    pub stop_all_account_io: qt_method!(fn(&mut self)),
    /// Stop-IO result, for tests.
    pub io_stopped: qt_signal!(success: bool, error: QString),

    /// Core `maybe_network` hint: wi-fi/mobile switch kills connection silently, noticed only at
    /// IDLE timeout (5 min); this reconnects now.
    pub maybe_network: qt_method!(fn(&mut self)),

    /// Default chatmail server's `dcaccount:` payload.
    pub default_provider_qr: qt_method!(fn(&mut self) -> QString),

    /// Create profile on chatmail server; core mints address. Empty `provider_qr`: core picks
    /// relays. Results: `profile_created`, `profile_error`, `profile_timed_out`; latest attempt
    /// only (`signup.rs`).
    pub create_profile: qt_method!(fn(&mut self, display_name: QString, provider_qr: QString)),

    /// Configure existing mailbox as transport.
    pub create_profile_with_email:
        qt_method!(fn(&mut self, display_name: QString, addr: QString, password: QString)),

    /// Profile ready; `account_id` has working transport.
    pub profile_created: qt_signal!(account_id: u32),
    /// Profile creation failed; core's message.
    pub profile_error: qt_signal!(message: QString),
    /// Relay silent for `seconds`; attempt abandoned, core process stopped, late profile removed.
    pub profile_timed_out: qt_signal!(seconds: u32),
    /// Seconds before abandon; 0 = thirty (`signup::DEADLINE`). Only tests set it.
    pub profile_timeout: qt_property!(u32; NOTIFY profile_timeout_changed),
    /// [`DeltaChatCore::profile_timeout`] changed.
    pub profile_timeout_changed: qt_signal!(),

    /// Configure progress for `account_id`: 0 failure, 1..=999 permille, 1000 done.
    pub configure_progress: qt_signal!(account_id: u32, permille: u32),

    /// Abort running attempt. No account id: onboarding has none; attempt knows its own.
    pub cancel_ongoing: qt_method!(fn(&mut self)),

    /// Take profile over from device showing this code.
    pub restore_from_device: qt_method!(fn(&mut self, qr_text: QString)),

    /// Take profile over from backup file.
    pub restore_from_file: qt_method!(fn(&mut self, path: QString)),

    /// Takeover progress, permille. 0: core gave up.
    pub restore_progress: qt_signal!(permille: u32),

    /// Profile on device, IO started.
    pub profile_restored: qt_signal!(account_id: u32),

    /// Takeover refused: `not-a-backup`, `too-new`, `stalled`.
    pub restore_refused: qt_signal!(reason: QString),

    /// Takeover refused, core's words.
    pub restore_failed: qt_signal!(message: QString),

    /// Add relay from `dcaccount:`, kept beside others (`Transports`). Results: `relay_added`,
    /// `relay_error`, `relay_timed_out`; latest attempt only. Progress on `configure_progress`;
    /// `cancel_ongoing` aborts. Late relay kept, not undone (`signup.rs`).
    pub add_relay: qt_method!(fn(&mut self, account_id: u32, provider_qr: QString)),
    /// Relay answered; one more transport.
    pub relay_added: qt_signal!(account_id: u32),
    /// Adding relay failed. Core's message.
    pub relay_error: qt_signal!(message: QString),
    /// Relay silent for `seconds`; attempt abandoned.
    pub relay_timed_out: qt_signal!(seconds: u32),

    /// Classify QR. `qr_checked`: upstream `Qr` JSON, `kind` `camelCase`, fields `snake_case`.
    pub check_qr: qt_method!(fn(&mut self, account_id: u32, qr_content: QString)),
    /// QR classified.
    pub qr_checked: qt_signal!(account_id: u32, kind: QString, payload_json: QString),
    /// Classifying QR failed.
    pub qr_error: qt_signal!(message: QString),

    /// Core `Error` event: message for user. Typed, so pages skip parsing.
    pub core_error: qt_signal!(message: QString),

    /// Reader's words for core text ("Draft", "Saved messages", errors): JSON stock id to
    /// translation (`qml/js/StockStrings.js`). Kept, given to each spawned server first, else text
    /// written to DB on arrival stays English.
    pub set_stock_strings: qt_method!(fn(&mut self, strings_json: QString)),

    rpc: Option<Arc<RpcClient>>,
    runtime: Option<CoreRuntime>,

    /// Server binary from `start`, for restart.
    rpc_server_path: String,
    /// Accounts with IO resumed; replayed after restart (fresh server runs none).
    io_accounts: BTreeSet<u32>,
    /// IO resumed for all accounts; repeated after restart.
    io_all: bool,
    /// Restarts since last healthy run; drives backoff.
    restart_attempt: u32,
    /// When current server spawned, for `HEALTHY_UPTIME`.
    server_started_at: Option<Instant>,
    /// True once a spawn succeeded. Before: failure is app failing to start.
    supervising: bool,
    /// True once QML gave a download limit. Before: default nobody chose.
    download_limit_set: bool,
    /// Same, deletion period.
    delete_device_after_set: bool,
    /// Same, media quality.
    media_quality_set: bool,
    /// Same, whose calls ring.
    who_can_call_me_set: bool,
    /// `write_config`'s writes per account and key, so list re-reads write only what's missing
    /// (writes cost; `delete_device_after` starts deletion pass). Cleared with server: old writes
    /// may not have landed.
    written_config: RefCell<BTreeMap<(u32, &'static str), String>>,
    /// Settings writes, one at a time in order asked: as separate tasks a later choice could
    /// land first and core keep older (two runtime threads; server answers out of order).
    /// Dropped with server.
    config_lane: RefCell<Option<ConfigLane>>,
    /// Profiles with unread read running, each with 'asked since' flag. One at a time: a message
    /// is 3-4 events; each read returns every unread id.
    unread_reads: RefCell<BTreeMap<u32, bool>>,
    /// Attempts per profile, shared with their tasks.
    attempts: Arc<Attempts>,
    /// Last `set_stock_strings` input.
    stock_strings: StockStrings,
}

/// Core's stock strings by id, reader's language.
type StockStrings = Arc<Mutex<BTreeMap<u32, String>>>;

impl DeltaChatCore {
    /// Spawn server, drain events. See `start`.
    pub fn start(&mut self, rpc_server_path: QString) {
        if self.runtime.is_some() {
            return;
        }

        // Own thread; see `crate::runtime`.
        let runtime = match CoreRuntime::new() {
            Ok(runtime) => runtime,
            Err(err) => {
                self.status = format!("error: failed to start async runtime: {err}").into();
                self.status_changed();
                return;
            }
        };

        let path = rpc_server_path.to_string();
        self.rpc_server_path.clone_from(&path);
        self.runtime = Some(runtime.clone());
        self.restart_attempt = 0;
        self.supervising = false;

        self.status = QString::from("starting");
        self.status_changed();

        let stock = self.stock_strings.clone();
        Self::spawn_server(QPointer::from(&*self), path, runtime, stock);
    }

    /// Spawn server, wire result; for `start` and restarts. Associated fn: QML's `start` holds a
    /// mutable borrow and `status_changed` handler calls straight back in, so mutations stay in
    /// scoped borrows, signals emit with none held.
    fn spawn_server(ptr: QPointer<Self>, path: String, runtime: CoreRuntime, stock: StockStrings) {
        let started_ptr = ptr;
        let retry_path = path.clone();
        let started = queued_callback(move |result: Result<Arc<RpcClient>, String>| {
            let Some(this) = started_ptr.as_pinned() else {
                return;
            };
            match result {
                Ok(rpc) => {
                    let (runtime, accounts, all) = {
                        let mut this_mut = this.borrow_mut();
                        this_mut.rpc = Some(rpc.clone());
                        this_mut.status = QString::from("ready");
                        this_mut.server_started_at = Some(Instant::now());
                        this_mut.supervising = true;
                        set_connection(this_mut.runtime.clone().map(|rt| (rpc.clone(), rt)));
                        (
                            this_mut.runtime.clone(),
                            this_mut.io_accounts.clone(),
                            this_mut.io_all,
                        )
                    };
                    // Drain first, or IO events go nowhere.
                    if let Some(runtime) = runtime {
                        Self::forward_events(started_ptr.clone(), rpc, runtime, retry_path.clone());
                    }
                    if all {
                        Self::resume_io(started_ptr.clone(), None);
                    }
                    for account_id in accounts {
                        Self::resume_io(started_ptr.clone(), Some(account_id));
                    }
                    // Last: handler may call back in.
                    this.borrow().status_changed();
                }
                Err(err) => {
                    // Only a server that worked once is retried.
                    if this.borrow().supervising {
                        Self::schedule_restart(
                            started_ptr.clone(),
                            retry_path.clone(),
                            Some(format!("could not restart the core: {err}")),
                        );
                        return;
                    }
                    // Never started: app failing to start.
                    {
                        // Dropped so later `start()` isn't blocked.
                        let mut this_mut = this.borrow_mut();
                        this_mut.runtime = None;
                        set_connection(None);
                        this_mut.status = format!("error: {err}").into();
                    }
                    this.borrow().status_changed();
                }
            }
        });

        runtime.spawn(async move {
            let result = async {
                let accounts_dir = Self::accounts_dir()?;
                RpcClient::spawn_with_env(
                    path,
                    Vec::<&str>::new(),
                    [("DC_ACCOUNTS_PATH", accounts_dir)],
                )
                .await
                .map(Arc::new)
                .map_err(|err| err.to_string())
            }
            .await;
            if let Ok(rpc) = &result {
                send_stock_strings(rpc, &stock).await;
            }
            started(result);
        });
    }

    /// Server gone: `reconnecting` + respawn after backoff, or `stopped`. `failure` (spawn error,
    /// last words, or `None`) reported on give-up.
    fn schedule_restart(ptr: QPointer<Self>, path: String, failure: Option<String>) {
        let Some(this) = ptr.as_pinned() else { return };
        set_connection(None);

        let next = {
            let mut this_mut = this.borrow_mut();
            this_mut.rpc = None;
            // In-flight work died with server; next starts clean.
            this_mut.written_config.get_mut().clear();
            *this_mut.config_lane.get_mut() = None;
            this_mut.unread_reads.get_mut().clear();
            // Nothing to restart: `start` never called, or gave up.
            let Some(runtime) = this_mut.runtime.clone() else {
                return;
            };
            // Long uptime resets backoff.
            if this_mut
                .server_started_at
                .is_some_and(|at| at.elapsed() >= HEALTHY_UPTIME)
            {
                this_mut.restart_attempt = 0;
            }
            this_mut.server_started_at = None;

            if this_mut.restart_attempt >= RESTART_LIMIT {
                // Drop runtime, or later `start()` no-ops.
                this_mut.runtime = None;
                // `stopped`, not `error:`: pages have a message for it. Reason on `core_error`.
                this_mut.status = QString::from("stopped");
                None
            } else {
                let delay = restart_delay(this_mut.restart_attempt);
                this_mut.restart_attempt = this_mut.restart_attempt.saturating_add(1);
                this_mut.status = QString::from("reconnecting");
                Some((delay, runtime, this_mut.stock_strings.clone()))
            }
        };
        this.borrow().status_changed();

        let Some((delay, runtime, stock)) = next else {
            if let Some(detail) = failure {
                this.borrow().core_error(detail.into());
            }
            return;
        };
        let spawn_runtime = runtime.clone();
        let retry = queued_callback(move |()| {
            Self::spawn_server(
                ptr.clone(),
                path.clone(),
                spawn_runtime.clone(),
                stock.clone(),
            );
        });
        runtime.spawn(async move {
            tokio::time::sleep(delay).await;
            retry(());
        });
    }

    /// Resume IO after restart, without `io_started` signal: nothing asked.
    fn resume_io(ptr: QPointer<Self>, account_id: Option<u32>) {
        let Some(this) = ptr.as_pinned() else { return };
        let Some((rpc, runtime)) = this.borrow().connection() else {
            return;
        };
        let failed = queued_callback(move |err: String| {
            if let Some(this) = ptr.as_pinned() {
                this.borrow().core_error(err.into());
            }
        });
        runtime.spawn(async move {
            if let Err(err) = start_io(&rpc, account_id).await {
                failed(err);
            }
        });
    }

    /// Account dir: `PIIRIT_ACCOUNTS_DIR`, else inside sailjail's grant,
    /// `~/.local/share/<OrganizationName>/<ApplicationName>`; piirit.desktop declares both as
    /// `piirit`. Old name Postivene: profiles looked up once.
    ///
    /// # Errors
    ///
    /// No `XDG_DATA_HOME` or `HOME`, or not creatable.
    pub fn accounts_dir() -> Result<String, String> {
        let dir = if let Ok(dir) = std::env::var("PIIRIT_ACCOUNTS_DIR") {
            std::path::PathBuf::from(dir)
        } else {
            let base = std::env::var("XDG_DATA_HOME")
                .map(std::path::PathBuf::from)
                .or_else(|_| {
                    std::env::var("HOME")
                        .map(|home| std::path::PathBuf::from(home).join(".local/share"))
                })
                .map_err(|err| format!("neither XDG_DATA_HOME nor HOME is set: {err}"))?;
            let dir = base.join("piirit/piirit/accounts");
            Self::adopt_legacy_accounts(&base.join("postivene/postivene/accounts"), &dir);
            dir
        };
        // Private (keys, mail password); mode set on existing dirs too.
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&dir)
            .map_err(|err| format!("cannot create accounts dir {}: {err}", dir.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|err| format!("cannot restrict accounts dir {}: {err}", dir.display()))?;
        }
        Ok(dir.to_string_lossy().into_owned())
    }

    /// Move profile from old app name, once. Best effort: confined app can't see old dir. Never
    /// overwrites.
    fn adopt_legacy_accounts(legacy: &std::path::Path, wanted: &std::path::Path) {
        if wanted.exists() || !legacy.is_dir() {
            return;
        }
        let Some(parent) = wanted.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_ok() {
            // Failure leaves old dir intact.
            if let Err(err) = std::fs::rename(legacy, wanted) {
                eprintln!(
                    "piirit: could not move the old profiles from {} to {}: {err}",
                    legacy.display(),
                    wanted.display()
                );
            }
        }
    }

    /// Forward `is_relayed` events to `relay`, one queued callback per batch (Qt 5.6: each
    /// costly). One `runtime.spawn`: `spawn_event_loop` needs runtime context.
    fn forward_events(
        ptr: QPointer<Self>,
        rpc: Arc<RpcClient>,
        runtime: CoreRuntime,
        path: String,
    ) {
        // Stream ends only when server dies; next server starts here.
        let stopped_ptr = ptr.clone();
        let stopped = queued_callback(move |tail: Vec<String>| {
            Self::schedule_restart(stopped_ptr.clone(), path.clone(), last_words(&tail));
        });
        let emit = queued_callback(move |batch: Vec<CoreEvent>| {
            for event in &batch {
                // Looked up per event: handler may remove the object.
                let Some(this) = ptr.as_pinned() else { return };
                this.borrow().relay(event);
            }
        });
        runtime.spawn(async move {
            let (mut events, _handle) = spawn_event_loop(rpc.clone());
            let mut arrived = Vec::new();
            while events.recv_many(&mut arrived, EVENT_BATCH_LIMIT).await > 0 {
                let batch: Vec<CoreEvent> = arrived
                    .drain(..)
                    .filter(|event| {
                        let kind = json::str_at(&event.event, "kind");
                        // Here, before filter (drops `SmtpMessageSent`): read at exit, after Qt
                        // thread stopped.
                        crate::call_host::note_sent(kind);
                        is_relayed(kind)
                    })
                    .collect();
                if !batch.is_empty() {
                    emit(batch);
                }
            }
            // Server gone; stderr only clue.
            stopped(rpc.stderr_tail());
        });
    }

    /// One event as signals: `core_event` for read kinds, typed signals for some.
    fn relay(&self, event: &CoreEvent) {
        let kind = match json::str_at(&event.event, "kind") {
            "" => "Unknown".to_string(),
            kind => kind.to_string(),
        };
        if kind == "Error" {
            let text = match json::str_at(&event.event, "msg") {
                "" => "the core reported an error",
                text => text,
            };
            self.core_error(text.into());
        }
        if kind == "ConfigureProgress" {
            if let Some(permille) = json::u32_opt(&event.event, "progress") {
                self.configure_progress(event.context_id, permille);
            }
        }
        // Takeover reported like configure; account is page's business.
        if kind == "ImexProgress" {
            if let Some(permille) = json::u32_opt(&event.event, "progress") {
                self.restore_progress(permille);
            }
        }
        if UNREAD_EVENT_KINDS.contains(&kind.as_str()) {
            self.refresh_unread(event.context_id);
        }
        // Fan-out only read kinds: per-listener serialising costs with screen off.
        if !HANDLED_EVENT_KINDS.contains(&kind.as_str()) {
            return;
        }
        let payload = serde_json::to_string(&event.event).unwrap_or_default();
        self.core_event(event.context_id, kind.into(), payload.into());
    }

    /// `get_system_info` round trip into [`DeltaChatCore::system_info`].
    pub fn check_health(&mut self) {
        let Some((rpc, runtime)) = self.connection() else {
            self.core_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<String, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(info) => {
                    this.borrow_mut().system_info = info.into();
                    this.borrow().system_info_changed();
                }
                // Own signal: timed-out check isn't a gone core; ready-gated buttons would die.
                Err(err) => this.borrow().core_error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call_unit::<BTreeMap<String, String>>("get_system_info")
                .await
                .map(|info| serde_json::to_string(&info).unwrap_or_default())
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Create unconfigured account.
    pub fn add_account(&mut self) {
        let Some((rpc, runtime)) = self.connection() else {
            self.account_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<u32, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(account_id) => this.borrow().account_added(account_id),
                Err(err) => this.borrow().account_error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call_unit::<u32>("add_account")
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Delete profile. Refreshes after; list decides if a profile remains.
    pub fn remove_account(&mut self, account_id: u32) {
        let Some((rpc, runtime)) = self.connection() else {
            self.account_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => this.borrow_mut().refresh_accounts(),
                Err(err) => this.borrow().account_error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("remove_account", (account_id,))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Set download limit, all accounts.
    pub fn set_download_limit(&mut self, bytes: u32) {
        if self.download_limit_set && self.download_limit == bytes {
            return;
        }
        self.download_limit = bytes;
        self.download_limit_set = true;
        self.download_limit_changed();
        self.spread("download_limit", bytes.to_string());
    }

    /// Set media quality, all accounts.
    pub fn set_media_quality(&mut self, quality: u32) {
        if self.media_quality_set && self.media_quality == quality {
            return;
        }
        self.media_quality = quality;
        self.media_quality_set = true;
        self.media_quality_changed();
        self.spread("media_quality", quality.to_string());
    }

    /// Set whose calls ring, all accounts.
    pub fn set_who_can_call_me(&mut self, who: u32) {
        if self.who_can_call_me_set && self.who_can_call_me == who {
            return;
        }
        self.who_can_call_me = who;
        self.who_can_call_me_set = true;
        self.who_can_call_me_changed();
        self.spread("who_can_call_me", who.to_string());
    }

    /// Set deletion period, all accounts.
    pub fn set_delete_device_after(&mut self, seconds: u32) {
        if self.delete_device_after_set && self.delete_device_after == seconds {
            return;
        }
        self.delete_device_after = seconds;
        self.delete_device_after_set = true;
        self.delete_device_after_changed();
        self.spread("delete_device_after", seconds.to_string());
    }

    /// Write one setting to current accounts. Before core up: `refresh_accounts`.
    fn spread(&self, key: &'static str, value: String) {
        let ptr: QPointer<Self> = QPointer::from(self);
        let done = queued_callback(move |result: Result<Vec<u32>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(ids) => this.borrow().write_config(&ids, key, value.clone()),
                Err(err) => this.borrow().core_error(err.into()),
            }
        });
        // In lane: callbacks queue in order asked, so their writes do too.
        self.in_config_lane(Box::new(move |rpc| {
            Box::pin(async move {
                done(account_ids(&rpc).await);
            })
        }));
    }

    /// Run `job` after every settings job asked before it has finished. No server: dropped.
    fn in_config_lane(&self, job: ConfigJob) {
        let mut lane = self.config_lane.borrow_mut();
        if lane.is_none() {
            let Some((rpc, runtime)) = self.connection() else {
                return;
            };
            let (sender, mut jobs) = tokio::sync::mpsc::unbounded_channel::<ConfigJob>();
            runtime.spawn(async move {
                while let Some(job) = jobs.recv().await {
                    job(rpc.clone()).await;
                }
            });
            *lane = Some(sender);
        }
        if let Some(sender) = lane.as_ref() {
            // Closed only with runtime gone: nothing left to write to.
            sender.send(job).ok();
        }
    }

    /// Write settings to accounts lacking them. `ids`: whole list; gone ones forgotten.
    fn apply_settings(&self, ids: &[u32]) {
        self.written_config
            .borrow_mut()
            .retain(|(account_id, _), _| ids.contains(account_id));
        if self.download_limit_set {
            self.write_config(ids, "download_limit", self.download_limit.to_string());
        }
        if self.delete_device_after_set {
            self.write_config(
                ids,
                "delete_device_after",
                self.delete_device_after.to_string(),
            );
        }
        if self.media_quality_set {
            self.write_config(ids, "media_quality", self.media_quality.to_string());
        }
        if self.who_can_call_me_set {
            self.write_config(ids, "who_can_call_me", self.who_can_call_me.to_string());
        }
    }

    /// Write value to each account not last given it; failure reported, others written, failed one
    /// retried next time. Remembered, not `get_config`: per-key reads would eat the saving.
    fn write_config(&self, ids: &[u32], key: &'static str, value: String) {
        if self.connection().is_none() {
            return;
        }
        let ids: Vec<u32> = {
            let mut written = self.written_config.borrow_mut();
            ids.iter()
                .copied()
                .filter(|&account_id| {
                    let unchanged = written.get(&(account_id, key)) == Some(&value);
                    if !unchanged {
                        written.insert((account_id, key), value.clone());
                    }
                    !unchanged
                })
                .collect()
        };
        if ids.is_empty() {
            return;
        }
        let ptr: QPointer<Self> = QPointer::from(self);
        let tried = value.clone();
        let failed = queued_callback(move |(account_id, err): (u32, String)| {
            let Some(this) = ptr.as_pinned() else { return };
            // Forgotten: next read retries, unless newer write kept.
            let this_ref = this.borrow();
            {
                let mut written = this_ref.written_config.borrow_mut();
                if written.get(&(account_id, key)) == Some(&tried) {
                    written.remove(&(account_id, key));
                }
            }
            this_ref.core_error(err.into());
        });
        self.in_config_lane(Box::new(move |rpc| {
            Box::pin(async move {
                for account_id in ids {
                    if let Err(err) = rpc
                        .call::<_, ()>("set_config", (account_id, key, Some(value.clone())))
                        .await
                    {
                        failed((account_id, format!("could not set {key}: {err}")));
                    }
                }
            })
        }));
    }

    /// Count what deletion period would delete now, all accounts.
    pub fn estimate_auto_deletion(&mut self, seconds: u32) {
        let Some((rpc, runtime)) = self.connection() else {
            self.core_error(QString::from("not started"));
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<u32, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(count) => this.borrow().auto_deletion_estimated(seconds, count),
                Err(err) => this.borrow().core_error(err.into()),
            }
        });
        runtime.spawn(async move {
            let result = async {
                let mut total: u64 = 0;
                for account_id in account_ids(&rpc).await? {
                    // From this device: setting is device's.
                    let count: u64 = rpc
                        .call(
                            "estimate_auto_deletion_count",
                            (account_id, false, i64::from(seconds)),
                        )
                        .await
                        .map_err(|err| err.to_string())?;
                    total = total.saturating_add(count);
                }
                Ok::<_, String>(u32::try_from(total).unwrap_or(u32::MAX))
            }
            .await;
            done(result);
        });
    }

    /// Repopulate [`DeltaChatCore::account_list`].
    pub fn refresh_accounts(&mut self) {
        let Some((rpc, runtime)) = self.connection() else {
            self.account_error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(
            move |result: Result<(Vec<AccountItem>, Option<u32>), String>| {
                let Some(this) = ptr.as_pinned() else { return };
                match result {
                    Ok((items, selected)) => {
                        // Saturating: "very many" is right for has-any-configured.
                        let configured_count =
                            u32::try_from(items.iter().filter(|item| item.is_configured).count())
                                .unwrap_or(u32::MAX);
                        let resume_account_id = resume_account(&items, selected);
                        let ids: Vec<u32> = items.iter().map(|item| item.account_id).collect();
                        reconcile_accounts(&mut this.borrow_mut().account_list.borrow_mut(), items);
                        this.borrow().apply_settings(&ids);
                        this.borrow()
                            .accounts_refreshed(configured_count, resume_account_id);
                    }
                    Err(err) => this.borrow().account_error(err.into()),
                }
            },
        );

        runtime.spawn(async move {
            let result = async {
                // Upstream `Account`: `kind` "Configured"/"Unconfigured", camelCase.
                let accounts: Vec<serde_json::Value> = rpc
                    .call_unit("get_all_accounts")
                    .await
                    .map_err(|err| err.to_string())?;
                // Core's selected profile, on disk. Null if never selected.
                let selected: Option<u32> = rpc
                    .call_unit("get_selected_account_id")
                    .await
                    .unwrap_or(None);
                let mut items: Vec<AccountItem> = accounts
                    .iter()
                    .filter_map(|account| {
                        Some(AccountItem {
                            account_id: json::u32_opt(account, "id")?,
                            display_name: json::text(account, "displayName"),
                            // Filled below (`own_address`).
                            addr: QString::default(),
                            is_configured: json::str_at(account, "kind") == "Configured",
                            avatar_path: json::text(account, "profileImage"),
                            color: json::text(account, "color"),
                            unread_count: 0,
                        })
                    })
                    .collect();
                // Two calls per configured profile: address, badge.
                for item in items.iter_mut().filter(|item| item.is_configured) {
                    if let Some(addr) = own_address(&rpc, item.account_id).await {
                        item.addr = addr.into();
                    }
                    item.unread_count = fresh_count(&rpc, item.account_id).await;
                }
                Ok::<_, String>((items, selected))
            }
            .await;
            done(result);
        });
    }

    /// Configured, per last read?
    pub fn is_configured_account(&self, account_id: u32) -> bool {
        self.account_list
            .borrow()
            .iter()
            .any(|row| row.account_id == account_id && row.is_configured)
    }

    /// Tell core shown profile.
    pub fn select_account(&mut self, account_id: u32) {
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = self.connection() else {
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let failed = queued_callback(move |err: String| {
            if let Some(this) = ptr.as_pinned() {
                this.borrow().account_error(err.into());
            }
        });
        runtime.spawn(async move {
            if let Err(err) = rpc.call::<_, ()>("select_account", (account_id,)).await {
                failed(format!("could not remember the profile: {err}"));
            }
        });
    }

    /// Re-read profile's unread count into its row; asks mid-read rerun after.
    pub fn refresh_unread(&self, account_id: u32) {
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = self.connection() else {
            return;
        };
        {
            let mut reads = self.unread_reads.borrow_mut();
            if let Some(again) = reads.get_mut(&account_id) {
                *again = true;
                return;
            }
            reads.insert(account_id, false);
        }
        let ptr: QPointer<Self> = QPointer::from(self);
        let done = queued_callback(move |count: u32| {
            let Some(this) = ptr.as_pinned() else { return };
            let this_ref = this.borrow();
            {
                let mut rows = this_ref.account_list.borrow_mut();
                // Read first: iterator borrows rows.
                let changed = rows
                    .iter()
                    .enumerate()
                    .find(|(_, row)| row.account_id == account_id && row.unread_count != count)
                    .map(|(index, row)| (index, row.clone()));
                if let Some((index, mut row)) = changed {
                    row.unread_count = count;
                    rows.change_line(index, row);
                }
            }
            let again = this_ref
                .unread_reads
                .borrow_mut()
                .remove(&account_id)
                .unwrap_or(false);
            if again {
                this_ref.refresh_unread(account_id);
            }
        });
        runtime.spawn(async move {
            done(fresh_count(&rpc, account_id).await);
        });
    }

    /// Resume IO, one configured account.
    pub fn start_account_io(&mut self, account_id: u32) {
        // Remembered before call: restart must resume what was asked.
        self.io_accounts.insert(account_id);
        let Some((rpc, runtime)) = self.connection() else {
            self.io_started(account_id, false, QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: (u32, Result<(), String>)| {
            let Some(this) = ptr.as_pinned() else { return };
            let (account_id, result) = result;
            match result {
                Ok(()) => this
                    .borrow()
                    .io_started(account_id, true, QString::default()),
                Err(err) => this.borrow().io_started(account_id, false, err.into()),
            }
        });

        runtime.spawn(async move {
            let result = start_io(&rpc, Some(account_id)).await;
            done((account_id, result));
        });
    }

    /// Resume IO, all configured accounts.
    pub fn start_all_account_io(&mut self) {
        // Remembered before call, as above.
        self.io_all = true;
        let Some((rpc, runtime)) = self.connection() else {
            self.io_started(0, false, QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => this.borrow().io_started(0, true, QString::default()),
                Err(err) => this.borrow().io_started(0, false, err.into()),
            }
        });

        runtime.spawn(async move {
            done(start_io(&rpc, None).await);
        });
    }

    /// Stop IO for all accounts.
    pub fn stop_all_account_io(&mut self) {
        // Forgotten before call: offline restart must not run IO.
        self.io_all = false;
        self.io_accounts.clear();
        let Some((rpc, runtime)) = self.connection() else {
            self.io_stopped(false, QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => this.borrow().io_stopped(true, QString::default()),
                Err(err) => this.borrow().io_stopped(false, err.into()),
            }
        });

        runtime.spawn(async move {
            done(
                rpc.call_unit::<()>("stop_io_for_all_accounts")
                    .await
                    .map_err(|err| err.to_string()),
            );
        });
    }

    /// Network hint. Best effort; dropped before core up.
    pub fn maybe_network(&mut self) {
        let Some((rpc, runtime)) = self.connection() else {
            return;
        };
        runtime.spawn(async move {
            rpc.call_unit::<()>("maybe_network").await.ok();
        });
    }

    /// `dcaccount:` payload, default chatmail server.
    pub fn default_provider_qr(&mut self) -> QString {
        QString::from(DEFAULT_PROVIDER_QR)
    }

    /// Create profile on chatmail, or core-picked relays (`init_transports`).
    pub fn create_profile(&mut self, display_name: QString, provider_qr: QString) {
        let transport = match provider_qr.to_string() {
            qr if qr.is_empty() => Transport::Automatic,
            qr => Transport::Qr(qr),
        };
        self.begin_profile(display_name.to_string(), transport);
    }

    /// Create profile on existing mailbox: `addr` and `password` only (docs/PROJECT.md).
    pub fn create_profile_with_email(
        &mut self,
        display_name: QString,
        addr: QString,
        password: QString,
    ) {
        self.begin_profile(
            display_name.to_string(),
            Transport::Mailbox {
                addr: addr.to_string(),
                password: password.to_string(),
            },
        );
    }

    /// Shared start of `create_profile*`.
    fn begin_profile(&mut self, display_name: String, transport: Transport) {
        let Some((rpc, runtime)) = self.connection() else {
            self.profile_error(QString::from("not started"));
            return;
        };
        let attempt = self.attempts.begin(self.deadline());
        let done = self.profile_callback(attempt.id());
        let task_runtime = runtime.clone();
        runtime.spawn(async move {
            done(
                attempt
                    .run(&task_runtime, rpc, display_name, transport)
                    .await,
            );
        });
    }

    /// Abort attempt: answers ignored; core process stopped for held accounts.
    pub fn cancel_ongoing(&mut self) {
        let held = self.attempts.cancel();
        let Some((rpc, runtime)) = self.connection() else {
            return;
        };
        runtime.spawn(async move {
            // Fire and forget: UI reacts to `ConfigureProgress(0)`.
            for account_id in held {
                rpc.call::<_, ()>("stop_ongoing_process", (account_id,))
                    .await
                    .ok();
            }
        });
    }

    /// Take over from device showing `qr_text`.
    pub fn restore_from_device(&mut self, qr_text: QString) {
        self.begin_restore(Source::Device(qr_text.to_string()));
    }

    /// Take over from backup at `path`.
    pub fn restore_from_file(&mut self, path: QString) {
        self.begin_restore(Source::File(path.to_string()));
    }

    /// Shared start of `restore_from_*`: one attempt at a time with signups.
    fn begin_restore(&mut self, source: Source) {
        let Some((rpc, runtime)) = self.connection() else {
            self.restore_failed(QString::from("not started"));
            return;
        };
        let attempt = self.attempts.begin(signup::TRANSFER_DEADLINE);
        let done = self.restore_callback(attempt.id());
        let task_runtime = runtime.clone();
        runtime.spawn(async move {
            done(attempt.restore(&task_runtime, rpc, source).await);
        });
    }

    /// Add relay from `dcaccount:` to `account_id`. One attempt at a time, as signups.
    pub fn add_relay(&mut self, account_id: u32, provider_qr: QString) {
        if account_id == 0 {
            self.relay_error(QString::from("no profile to add a relay to"));
            return;
        }
        let Some((rpc, runtime)) = self.connection() else {
            self.relay_error(QString::from("not started"));
            return;
        };
        let attempt = self.attempts.begin(self.deadline());
        // Before task: same-turn Cancel finds account.
        attempt.hold(account_id);
        let done = self.relay_callback(attempt.id());
        let task_runtime = runtime.clone();
        let transport = Transport::Qr(provider_qr.to_string());
        runtime.spawn(async move {
            done(
                attempt
                    .add_transport(&task_runtime, rpc, account_id, transport)
                    .await,
            );
        });
    }

    /// Relay timeout: `profile_timeout` if set, else thirty seconds.
    fn deadline(&self) -> Duration {
        match self.profile_timeout {
            0 => signup::DEADLINE,
            seconds => Duration::from_secs(u64::from(seconds)),
        }
    }

    /// Completion of `check_qr`.
    fn qr_callback(&self) -> impl Fn((u32, Result<serde_json::Value, String>)) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |result: (u32, Result<serde_json::Value, String>)| {
            let Some(this) = ptr.as_pinned() else { return };
            let (account_id, result) = result;
            match result {
                Ok(qr) => {
                    let kind = match json::str_at(&qr, "kind") {
                        "" => "unknown".to_string(),
                        kind => kind.to_string(),
                    };
                    let payload = serde_json::to_string(&qr).unwrap_or_default();
                    this.borrow()
                        .qr_checked(account_id, kind.into(), payload.into());
                }
                Err(err) => this.borrow().qr_error(err.into()),
            }
        })
    }

    /// Completion of `create_profile*`. Stale profile (relay answered after cancel) removed, not
    /// announced: else app opens on it next start.
    fn profile_callback(&self, attempt: u64) -> impl Fn(Outcome) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |outcome: Outcome| {
            let Some(this) = ptr.as_pinned() else { return };
            let wanted = this.borrow().attempts.is_current(attempt);
            match outcome {
                Outcome::Created(account_id) if wanted => {
                    this.borrow().profile_created(account_id);
                }
                Outcome::Created(account_id) => {
                    if let Some((rpc, runtime)) = this.borrow().connection() {
                        runtime.spawn(async move { signup::discard(&rpc, account_id).await });
                    }
                }
                Outcome::Failed(err) if wanted => this.borrow().profile_error(err.into()),
                Outcome::TimedOut(seconds) if wanted => {
                    this.borrow().profile_timed_out(seconds);
                }
                Outcome::Failed(_) | Outcome::TimedOut(_) => {}
            }
        })
    }

    /// Completion of `add_relay`. Not undone when stale.
    fn relay_callback(&self, attempt: u64) -> impl Fn(Outcome) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |outcome: Outcome| {
            let Some(this) = ptr.as_pinned() else { return };
            if !this.borrow().attempts.is_current(attempt) {
                return;
            }
            match outcome {
                Outcome::Created(account_id) => this.borrow().relay_added(account_id),
                Outcome::Failed(err) => this.borrow().relay_error(err.into()),
                Outcome::TimedOut(seconds) => this.borrow().relay_timed_out(seconds),
            }
        })
    }

    /// Completion of `restore_from_*`: stale profile removed. Starts IO here (import leaves none
    /// running) and repopulates list (page may be gone).
    fn restore_callback(&self, attempt: u64) -> impl Fn(Taken) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |taken: Taken| {
            let Some(this) = ptr.as_pinned() else { return };
            let wanted = this.borrow().attempts.is_current(attempt);
            match taken {
                Taken::Done(account_id) if wanted => {
                    if let Some((rpc, runtime)) = this.borrow().connection() {
                        runtime.spawn(async move {
                            if let Err(err) = start_io(&rpc, Some(account_id)).await {
                                eprintln!("piirit: starting IO for a restored profile: {err}");
                            }
                        });
                    }
                    this.borrow_mut().refresh_accounts();
                    this.borrow().profile_restored(account_id);
                }
                Taken::Done(account_id) => {
                    if let Some((rpc, runtime)) = this.borrow().connection() {
                        runtime.spawn(async move { signup::discard(&rpc, account_id).await });
                    }
                }
                Taken::Refused(reason) if wanted => {
                    this.borrow().restore_refused(reason.into());
                }
                Taken::Failed(err) if wanted => this.borrow().restore_failed(err.into()),
                Taken::Refused(_) | Taken::Failed(_) => {}
            }
        })
    }

    /// Transport and runtime, once [`DeltaChatCore::start`] done.
    fn connection(&self) -> Option<(Arc<RpcClient>, CoreRuntime)> {
        Some((self.rpc.clone()?, self.runtime.clone()?))
    }

    /// Keep stock strings; hand to running server.
    pub fn set_stock_strings(&mut self, strings_json: QString) {
        let strings = match parse_stock_strings(&strings_json.to_string()) {
            Ok(strings) => strings,
            Err(err) => {
                self.core_error(format!("stock strings: {err}").into());
                return;
            }
        };
        *self
            .stock_strings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = strings;
        let Some((rpc, runtime)) = self.connection() else {
            return;
        };
        let stock = self.stock_strings.clone();
        runtime.spawn(async move {
            send_stock_strings(&rpc, &stock).await;
        });
    }

    /// Classify QR via core.
    pub fn check_qr(&mut self, account_id: u32, qr_content: QString) {
        let Some((rpc, runtime)) = self.connection() else {
            self.qr_error(QString::from("not started"));
            return;
        };

        let done = self.qr_callback();

        let qr_content = qr_content.to_string();
        runtime.spawn(async move {
            let result = rpc
                .call::<_, serde_json::Value>("check_qr", (account_id, qr_content))
                .await
                .map_err(|err| err.to_string());
            done((account_id, result));
        });
    }
}

/// Read `set_stock_strings` input: id to text. Non-numeric key: error, not skipped.
fn parse_stock_strings(text: &str) -> Result<BTreeMap<u32, String>, String> {
    let map: BTreeMap<String, String> =
        serde_json::from_str(text).map_err(|err| err.to_string())?;
    map.into_iter()
        .map(|(id, value)| {
            id.parse::<u32>()
                .map(|id| (id, value))
                .map_err(|err| format!("{id:?} is not a stock string id: {err}"))
        })
        .collect()
}

/// Hand core stock strings. Refusal: core keeps English; stderr line only.
async fn send_stock_strings(rpc: &RpcClient, stock: &StockStrings) {
    let strings = stock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if strings.is_empty() {
        return;
    }
    if let Err(err) = rpc.call::<_, ()>("set_stock_strings", (strings,)).await {
        eprintln!("piirit: the core refused the stock strings: {err}");
    }
}

/// One settings job: runs with server's client, finished before next starts.
type ConfigJob = Box<
    dyn FnOnce(Arc<RpcClient>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send,
>;

/// Feeds the task running [`ConfigJob`]s in order. Dropped: task ends after jobs queued.
type ConfigLane = tokio::sync::mpsc::UnboundedSender<ConfigJob>;

/// Id of every account core has, configured or not.
async fn account_ids(rpc: &RpcClient) -> Result<Vec<u32>, String> {
    rpc.call_unit::<Vec<serde_json::Value>>("get_all_accounts")
        .await
        .map(|accounts| {
            accounts
                .iter()
                .filter_map(|account| json::u32_opt(account, "id"))
                .collect()
        })
        .map_err(|err| err.to_string())
}

/// Own address: own contact's (id 1), core 2.66+ first relay's. Account list lacks it since
/// 2.61; `configured_addr` gone in 2.66. `None` if core can't say (no relay yet).
pub(crate) async fn own_address(rpc: &RpcClient, account_id: u32) -> Option<String> {
    rpc.call::<_, serde_json::Value>("get_contact", (account_id, 1_u32))
        .await
        .ok()
        .map(|contact| json::str_at(&contact, "address").to_string())
        .filter(|addr| !addr.is_empty())
}

/// Unread messages across account's chats, via `get_fresh_msgs`, as reference clients badge.
/// Muted out; uncountable: zero.
async fn fresh_count(rpc: &RpcClient, account_id: u32) -> u32 {
    rpc.call::<_, Vec<u32>>("get_fresh_msgs", (account_id,))
        .await
        .map_or(0, |ids| u32::try_from(ids.len()).unwrap_or(u32::MAX))
}

/// Start IO for one account, or `None` for all.
async fn start_io(rpc: &RpcClient, account_id: Option<u32>) -> Result<(), String> {
    match account_id {
        Some(account_id) => rpc.call::<_, ()>("start_io", (account_id,)).await,
        None => rpc.call_unit::<()>("start_io_for_all_accounts").await,
    }
    .map_err(|err| err.to_string())
}

/// Profile to open on: core's selected (last shown, kept across runs) if configured, else first
/// configured, else 0.
fn resume_account(items: &[AccountItem], selected: Option<u32>) -> u32 {
    let usable = |item: &&AccountItem| item.is_configured;
    selected
        .and_then(|id| {
            items
                .iter()
                .filter(usable)
                .find(|item| item.account_id == id)
        })
        .or_else(|| items.iter().find(usable))
        .map_or(0, |item| item.account_id)
}

/// `reset_data` destroys delegates, and Silica remorse countdown for deletion lives on row: reset
/// killed other pending deletions. Rows edited in place instead.
fn reconcile_accounts(rows: &mut AccountListModel, wanted: Vec<AccountItem>) {
    reconcile_rows(rows, wanted, |item| item.account_id);
}

/// Remove, insert, change rows until `rows` holds `wanted`, matched by `id`.
pub(crate) fn reconcile_rows<T, K>(
    rows: &mut SimpleListModel<T>,
    wanted: Vec<T>,
    id: impl Fn(&T) -> K,
) where
    T: SimpleListItem + PartialEq,
    K: Ord,
{
    // Nothing to keep: one reset is cheaper.
    if rows.iter().next().is_none() {
        rows.reset_data(wanted);
        return;
    }
    let wanted_ids: BTreeSet<K> = wanted.iter().map(&id).collect();
    let gone: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !wanted_ids.contains(&id(row)))
        .map(|(index, _)| index)
        .collect();
    // Backwards: indices stay valid.
    for index in gone.into_iter().rev() {
        rows.remove(index);
    }
    let count = wanted.len();
    for (index, item) in wanted.into_iter().enumerate() {
        // Read first: iterator borrows rows.
        let current = rows.iter().nth(index).map(&id);
        match current {
            Some(current) if current == id(&item) => {
                if rows[index] != item {
                    rows.change_line(index, item);
                }
            }
            _ => rows.insert(index, item),
        }
    }
    // Only reorder leaves rows past end.
    while rows.iter().count() > count {
        rows.remove(count);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_relayed, last_words, reconcile_accounts, restart_delay, resume_account, server_path,
        AccountItem, AccountListModel, BUNDLED_SERVER, HANDLED_EVENT_KINDS, RESTART_DELAY_MAX,
        RESTART_DELAY_MIN, RESTART_LIMIT,
    };

    #[test]
    fn the_core_log_stays_on_the_tokio_side_and_the_rest_crosses() {
        for kind in ["Info", "Warning", "SmtpConnected", "ImapConnected", ""] {
            assert!(!is_relayed(kind), "{kind:?} would cross for nothing");
        }
        // Read by `core_event` or `relay`.
        for kind in HANDLED_EVENT_KINDS.iter().chain(&[
            "Error",
            "ConfigureProgress",
            "ImexProgress",
            "IncomingMsgBunch",
        ]) {
            assert!(is_relayed(kind), "{kind:?} would never reach the app");
        }
    }

    fn account(id: u32, name: &str) -> AccountItem {
        AccountItem {
            account_id: id,
            display_name: name.into(),
            is_configured: true,
            ..AccountItem::default()
        }
    }

    fn ids(rows: &AccountListModel) -> Vec<(u32, String)> {
        rows.iter()
            .map(|row| (row.account_id, row.display_name.to_string()))
            .collect()
    }

    #[test]
    fn a_refresh_removes_inserts_and_changes_rows_where_they_stand() {
        let mut rows = AccountListModel::default();
        reconcile_accounts(
            &mut rows,
            vec![account(1, "a"), account(2, "b"), account(3, "c")],
        );
        assert_eq!(
            ids(&rows),
            vec![(1, "a".into()), (2, "b".into()), (3, "c".into())]
        );
        // One gone, one renamed, one new.
        reconcile_accounts(
            &mut rows,
            vec![account(1, "ada"), account(3, "c"), account(4, "d")],
        );
        assert_eq!(
            ids(&rows),
            vec![(1, "ada".into()), (3, "c".into()), (4, "d".into())]
        );
        // Ends with exactly wanted rows.
        reconcile_accounts(&mut rows, vec![account(4, "d"), account(1, "ada")]);
        assert_eq!(ids(&rows), vec![(4, "d".into()), (1, "ada".into())]);
        reconcile_accounts(&mut rows, Vec::new());
        assert_eq!(ids(&rows), Vec::<(u32, String)>::new());
    }

    #[test]
    fn the_app_comes_back_to_the_selected_profile_when_it_is_usable() {
        let unconfigured = AccountItem {
            account_id: 3,
            is_configured: false,
            ..AccountItem::default()
        };
        let items = vec![account(1, "a"), account(2, "b"), unconfigured];
        // Core's selected profile wins.
        assert_eq!(resume_account(&items, Some(2)), 2);
        assert_eq!(resume_account(&items, Some(1)), 1);
        // Nothing selected, gone, or unfinished: first that works.
        assert_eq!(resume_account(&items, None), 1);
        assert_eq!(resume_account(&items, Some(9)), 1);
        assert_eq!(resume_account(&items, Some(3)), 1);
        // Nothing usable: nothing to return to.
        assert_eq!(resume_account(&[], Some(1)), 0);
        assert_eq!(resume_account(&items[2..], None), 0);
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn the_server_is_the_flag_then_the_environment_then_the_bundle() {
        assert_eq!(
            server_path(args(&["--rpc-server", "/x/server"]), Some("/env".into())),
            "/x/server"
        );
        assert_eq!(
            server_path(args(&["--rpc-server=/y/server"]), None),
            "/y/server"
        );
        // Invoker's own args are not a server.
        assert_eq!(
            server_path(
                args(&["-prestart", "--type=silica-qt5"]),
                Some("/env".into())
            ),
            "/env"
        );
        assert_eq!(server_path(args(&[]), None), BUNDLED_SERVER);
    }

    #[test]
    fn the_server_is_never_looked_up_on_path() {
        // Bare name would go to PATH, which gets mail password.
        for env in [None, Some(String::new())] {
            let path = server_path(args(&["--rpc-server"]), env);
            assert!(path.starts_with('/'), "{path:?} would be looked up on PATH");
        }
    }

    #[test]
    fn last_words_keep_the_tail_and_say_nothing_for_silence() {
        assert_eq!(last_words(&[]), None);
        let lines: Vec<String> = (1..=7).map(|n| format!("line {n}")).collect();
        let words = last_words(&lines).unwrap_or_default();
        assert!(words.ends_with("line 3 | line 4 | line 5 | line 6 | line 7"));
        assert!(!words.contains("line 2"));
    }

    #[test]
    fn the_backoff_doubles_and_then_stops_doubling() {
        assert_eq!(restart_delay(0), RESTART_DELAY_MIN);
        assert_eq!(restart_delay(1), RESTART_DELAY_MIN * 2);
        assert_eq!(restart_delay(2), RESTART_DELAY_MIN * 4);
        assert_eq!(restart_delay(5), RESTART_DELAY_MAX);
        // Later attempts wait ceiling; no overflow panic.
        assert_eq!(restart_delay(RESTART_LIMIT), RESTART_DELAY_MAX);
        assert_eq!(restart_delay(u32::MAX), RESTART_DELAY_MAX);
    }

    #[test]
    fn giving_up_takes_longer_than_anything_transient() {
        let total: std::time::Duration = (0..RESTART_LIMIT).map(restart_delay).sum();
        assert!(
            total >= std::time::Duration::from_secs(180),
            "the app gives up after {total:?}, which is not long enough to \
             outlast a phone that is briefly out of memory"
        );
    }
}
