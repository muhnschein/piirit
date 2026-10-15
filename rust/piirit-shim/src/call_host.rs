//! Loopback host for one call's page.
//!
//! Gecko does media; upstream calls-webapp (`vendor/calls-webapp/`) drives it. Page reaches
//! core (place, accept, end) via `window.calls`: served from here, its `calls.js` bridge's five
//! functions are requests back to this host (as `webxdc_host.rs`).
//!
//! Other way: [`Call`](crate::calls::Call) hears answer (core event, Qt thread), queues hash
//! command (`#onAnswer=...`) via [`Host::command`]; bridge long-polls, sets it on page.
//! Mic too ([`mute_command`]).
//!
//! Guards as webxdc host: 127.0.0.1, kernel port, `Host:` must match, core routes behind
//! unguessable token, nothing served after call, page compiled in.
//!
//! Token only in app's `WebView` address, not bridge: foreign page loading `calls.js` by script
//! tag learns nothing to answer or end a call.
//!
//! CSP: peer connection ungoverned (page's point); fetches, scripts, frames stay on origin.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use deltachat_jsonrpc::RpcClient;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::json;
use crate::webxdc_host::{content_type, read_body, read_head, token};

/// Upstream release build, verbatim; pinned by `scripts/fetch-calls-webapp.sh`. Compiled in:
/// file package could lose = call failing on phone only.
const PAGE: &str = include_str!("../../../vendor/calls-webapp/index.html");

/// `window.calls` JS: `call_video.js` (camera) then bridge.
const BRIDGE: &str = concat!(include_str!("call_video.js"), include_str!("calls.js"));

/// Page CSP: inline script/style, pictures and fetches on origin.
const POLICY: &str = "default-src 'self'; \
     script-src 'self' 'unsafe-inline'; \
     style-src 'self' 'unsafe-inline'; \
     img-src 'self' data: blob:; \
     media-src 'self' blob: mediastream:; \
     connect-src 'self'; \
     object-src 'none'; \
     base-uri 'none'; \
     form-action 'none'; \
     frame-ancestors 'none'";

/// Long-poll wait before empty answer; bridge re-asks at once.
const COMMAND_WAIT: Duration = Duration::from_secs(20);

/// Hosts for [`end_all`]; weak.
static HOSTS: Mutex<Vec<Weak<Shared>>> = Mutex::new(Vec::new());

/// Messages core is done sending (sent or given up); [`note_sent`].
static SENT: AtomicU64 = AtomicU64::new(0);

/// [`end_all`] poll interval for call-ending message to go.
const SENT_POLL: Duration = Duration::from_millis(50);

/// Host setup.
pub(crate) struct Setup {
    /// Account.
    pub(crate) account_id: u32,
    /// Chat outgoing call goes into.
    pub(crate) chat_id: u32,
    /// Call message; 0 for outgoing until core places it.
    pub(crate) message_id: u32,
    /// Outgoing call is video.
    pub(crate) has_video: bool,
    /// Core's ICE servers, JSON string.
    pub(crate) ice_servers: String,
    /// Other end's picture file; empty for none.
    pub(crate) avatar_path: String,
}

/// Result to call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Report {
    /// Core placed call as this message.
    Placed(u32),
    /// Core has page's answer to incoming call.
    Accepted,
    /// ICE state: `checking`, `connected`, `completed`, `disconnected`, `failed`, `closed`.
    Connection(String),
    /// Picture state from bridge.
    Video(Video),
    /// Page asked call end (red button).
    EndedByPage,
    /// Core refused, its words.
    Failed(String),
}

/// Bridge's picture report on `/video`.
// Independent flags from bridge, each read alone.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Video {
    /// Camera open and sending.
    pub(crate) local: bool,
    /// Other end's picture drawn.
    pub(crate) remote: bool,
    /// Camera painted since last given stream (not black), or given up.
    pub(crate) local_painted: bool,
    /// Same, other end.
    pub(crate) remote_painted: bool,
    /// Camera faces reader.
    pub(crate) front: bool,
    /// Last camera asked for failed.
    pub(crate) failed: bool,
    /// Engine error name and message; empty if none.
    pub(crate) error: String,
}

impl Video {
    /// Parse bridge JSON; missing field false, non-object None.
    pub(crate) fn from_json(text: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        value.is_object().then(|| Self {
            local: json::flag(&value, "local"),
            remote: json::flag(&value, "remote"),
            local_painted: json::flag(&value, "localPainted"),
            remote_painted: json::flag(&value, "remotePainted"),
            front: json::flag(&value, "front"),
            failed: json::flag(&value, "failed"),
            error: json::str_at(&value, "error").to_owned(),
        })
    }
}

/// Reports to call on Qt thread via queued callback (as `webxdc_host::HandedOver`).
pub(crate) type Reporter = Arc<dyn Fn(Report) + Send + Sync>;

/// Shared by every connection.
struct Shared {
    rpc: Arc<RpcClient>,
    account_id: u32,
    chat_id: u32,
    has_video: bool,
    ice_servers: String,
    avatar_path: String,
    /// Call message; 0 until known.
    message_id: AtomicU32,
    /// Core-route token: in page address, not served content.
    key: String,
    /// `/call-api/<key>`: core routes.
    api: String,
    /// `Host:` request must carry.
    authority: String,
    /// Cleared on drop: nothing served after.
    running: AtomicBool,
    /// This end done (red button or hang-up): place still en route ends on landing.
    ending: AtomicBool,
    /// `end_call` sent (once), or not needed ([`Host::settle`]).
    ended: AtomicBool,
    /// `end_call` sent from here.
    told: AtomicBool,
    /// Held while `end_call` runs: second asker (close just after hang-up) waits.
    ending_call: tokio::sync::Mutex<()>,
    /// Page asked call placed; second ask = second call.
    placing: AtomicBool,
    /// Hash commands awaiting bridge.
    commands: Mutex<VecDeque<String>>,
    /// Woken on queued command or drop.
    changed: Notify,
    report: Reporter,
    /// Page's log requests.
    log: CallLog,
}

impl Shared {
    fn queue(&self) -> std::sync::MutexGuard<'_, VecDeque<String>> {
        self.commands
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Tell core call over, once, if there is a call.
    async fn end(&self) {
        let _once = self.ending_call.lock().await;
        let message_id = self.message_id.load(Ordering::SeqCst);
        if message_id == 0 || self.ended.swap(true, Ordering::SeqCst) {
            return;
        }
        self.told.store(true, Ordering::SeqCst);
        let ended: Result<serde_json::Value, _> = self
            .rpc
            .call("end_call", (self.account_id, message_id))
            .await;
        if let Err(err) = ended {
            eprintln!("piirit: end_call: {err}");
        }
    }
}

/// Served call. Drop stops host, tells core if leaving.
pub(crate) struct Host {
    /// Page location, no query or command.
    page: String,
    shared: Arc<Shared>,
    task: JoinHandle<()>,
    /// Runs call-ending after drop (sync, on Qt thread).
    runtime: Handle,
}

impl Host {
    /// `WebView` URL: `options` and key in query, `command` as hash.
    pub(crate) fn url(&self, options: &str, command: &str) -> String {
        format!("{}?{options}&key={}#{command}", self.page, self.shared.key)
    }

    /// Command for bridge to collect.
    pub(crate) fn command(&self, command: String) {
        self.shared.queue().push_back(command);
        self.shared.changed.notify_waiters();
    }

    /// Core already knows call over: drop tells nothing.
    pub(crate) fn settle(&self) {
        self.shared.ended.store(true, Ordering::SeqCst);
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::SeqCst);
        self.shared.ending.store(true, Ordering::SeqCst);
        self.task.abort();
        // Answer waiting long poll now.
        self.shared.changed.notify_waiters();
        // Core hears once; if other end ended it, `end_call` is no-op.
        let shared = Arc::clone(&self.shared);
        drop(self.runtime.spawn(async move { shared.end().await }));
    }
}

/// Serve call's page on loopback until [`Host`] dropped.
///
/// # Errors
///
/// Fails if loopback port unbindable.
pub(crate) async fn start(
    rpc: Arc<RpcClient>,
    setup: Setup,
    report: Reporter,
) -> Result<Host, String> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .map_err(|err| format!("cannot serve the call: {err}"))?;
    let address = listener
        .local_addr()
        .map_err(|err| format!("cannot serve the call: {err}"))?;
    let authority = format!("127.0.0.1:{}", address.port());
    let page = format!("http://{authority}/index.html");
    let key = token();
    let shared = Arc::new(Shared {
        rpc,
        account_id: setup.account_id,
        chat_id: setup.chat_id,
        has_video: setup.has_video,
        ice_servers: setup.ice_servers,
        avatar_path: setup.avatar_path,
        message_id: AtomicU32::new(setup.message_id),
        api: format!("/call-api/{key}"),
        key,
        authority,
        running: AtomicBool::new(true),
        ending: AtomicBool::new(false),
        ended: AtomicBool::new(false),
        told: AtomicBool::new(false),
        ending_call: tokio::sync::Mutex::new(()),
        placing: AtomicBool::new(false),
        commands: Mutex::new(VecDeque::new()),
        changed: Notify::new(),
        report,
        log: CallLog::begin(crate::qr::cache_file(LOG_FILE).ok()),
    });
    shared.log.write(&media_plugins(
        std::env::var("MOZ_GMP_PATH").ok().as_deref(),
    ));
    shared.log.write(&format!(
        "sound server properties: {}",
        std::env::var(PULSE_PROP).unwrap_or_default()
    ));
    {
        let mut hosts = HOSTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        hosts.retain(|host| host.strong_count() > 0);
        hosts.push(Arc::downgrade(&shared));
    }
    let task = tokio::spawn(accept(listener, Arc::clone(&shared)));
    Ok(Host {
        page,
        shared,
        task,
        runtime: Handle::current(),
    })
}

/// End hosted calls at app exit, before core stops (which precedes Qt dropping hosts); caller
/// bounds wait. Ringing call (no host) left alone: close is no decline, other device may answer.
///
/// `end_call` only queues message; stopping core cuts sending. So waits for core to report a
/// message went, else other end rings on till its own timeout.
pub(crate) async fn end_all() {
    let sent_before = SENT.load(Ordering::SeqCst);
    let hosts: Vec<Arc<Shared>> = HOSTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter_map(Weak::upgrade)
        .collect();
    let mut told = false;
    for shared in hosts {
        // Call still being placed ends on landing.
        shared.ending.store(true, Ordering::SeqCst);
        shared.end().await;
        told |= shared.told.load(Ordering::SeqCst);
    }
    if !told {
        return;
    }
    while SENT.load(Ordering::SeqCst) == sent_before {
        tokio::time::sleep(SENT_POLL).await;
    }
}

/// Count send-ending core events for [`end_all`].
pub(crate) fn note_sent(kind: &str) {
    if matches!(kind, "SmtpMessageSent" | "MsgDelivered" | "MsgFailed") {
        SENT.fetch_add(1, Ordering::SeqCst);
    }
}

/// Page opens several connections, one a held long poll: task each.
async fn accept(listener: TcpListener, shared: Arc<Shared>) {
    while shared.running.load(Ordering::SeqCst) {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            // Failed connection: page gave up.
            answer(stream, shared).await.ok();
        });
    }
}

/// What goes back.
struct Response {
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn new(content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status: "200 OK",
            content_type,
            body,
        }
    }

    /// Refusal with own reason; see `webxdc_host`.
    fn empty(status: &'static str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: status.as_bytes().to_vec(),
        }
    }
}

/// Read request, answer, close.
async fn answer(mut stream: TcpStream, shared: Arc<Shared>) -> std::io::Result<()> {
    let response = match read_head(&mut stream).await? {
        None => Response::empty("400 Bad Request"),
        Some(head) => {
            let (method, target, host) =
                (head.method.clone(), head.target.clone(), head.host.clone());
            match read_body(&mut stream, head).await? {
                None => Response::empty("413 Payload Too Large"),
                Some(body) => route(&shared, &method, &target, &host, body).await,
            }
        }
    };
    write_response(&mut stream, &response).await
}

/// Max log line length.
const LOG_LINE_MAX: usize = 300;

/// Last call's log file in app cache.
const LOG_FILE: &str = "call.log";

/// Log line: `APIs:` per `MOZ_GMP_PATH` dir.
///
/// Gecko encodes H264 only via plugin (Sailfish: gmp-droid); `iPhone` takes nothing else.
fn media_plugins(path: Option<&str>) -> String {
    let Some(path) = path.filter(|path| !path.is_empty()) else {
        return "media plugins: MOZ_GMP_PATH is not set".to_string();
    };
    let found: Vec<String> = path
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| {
            let apis = plugin_apis(Path::new(dir));
            format!("{dir} {}", apis.as_deref().unwrap_or("(no APIs found)"))
        })
        .collect();
    format!("media plugins: {}", found.join("; "))
}

/// `APIs:` line of plugin dir's `.info`, e.g. `decode-video[h264:vp8], encode-video[h264]`.
fn plugin_apis(dir: &Path) -> Option<String> {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|file| file.extension().is_some_and(|ext| ext == "info"))
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .find_map(|info| {
            info.lines()
                .find_map(|line| line.strip_prefix("APIs:"))
                .map(|apis| apis.trim().to_string())
        })
}

/// `APIs:` offers H.264 encoder, parsed as Gecko (`GMPParent::ReadGMPInfoFile`): ", " then ":".
fn encodes_h264(apis: &str) -> bool {
    apis.split(", ").any(|capability| {
        capability
            .strip_prefix("encode-video[")
            .and_then(|tags| tags.strip_suffix(']'))
            .is_some_and(|tags| tags.split(':').any(|tag| tag == "h264"))
    })
}

/// Package's H.264 encoder plugin (`piirit-video/build.rs`).
const BUNDLED_ENCODER: &str = "/usr/share/harbour-piirit/lib/gmp-piirit/1.0";

/// `MOZ_GMP_PATH`: existing dirs then `ours`. `None` (left alone) if listed plugin encodes
/// H.264 (gmp-droid, session-wide; hardware beats software), or `ours` listed or absent.
fn encoder_path(existing: Option<&str>, ours: &Path) -> Option<String> {
    let ours_text = ours.to_str()?;
    if !ours.join("piirit.info").is_file() {
        return None;
    }
    let dirs: Vec<&str> = existing
        .unwrap_or("")
        .split(':')
        .filter(|dir| !dir.is_empty())
        .collect();
    if dirs
        .iter()
        .any(|&dir| plugin_apis(Path::new(dir)).is_some_and(|apis| encodes_h264(&apis)))
    {
        return None;
    }
    Some(
        dirs.into_iter()
            .chain(std::iter::once(ours_text))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

/// Before any thread: point Gecko at H.264 encoder (`encoder_path`), give it call log path
/// (plugin process inherits). `PIIRIT_GMP_DIR` overrides encoder. Ensures `PULSE_PROP`.
pub fn prepare_media_plugins() {
    let log = crate::qr::cache_file(LOG_FILE).ok();
    for (name, value) in media_environment(|name| std::env::var(name).ok(), log) {
        std::env::set_var(name, value);
    }
    keep_pulse_properties();
}

/// Ensure `PULSE_PROP` exists, keep value. Single-threaded only: see `label_call_audio`.
fn keep_pulse_properties() {
    let session = match std::env::var(PULSE_PROP) {
        Ok(session) => session,
        Err(std::env::VarError::NotPresent) => {
            std::env::set_var(PULSE_PROP, "");
            String::new()
        }
        // Not ours to read or overwrite.
        Err(std::env::VarError::NotUnicode(_)) => return,
    };
    SESSION_PULSE_PROP.get_or_init(|| session);
}

/// `PulseAudio` connection properties, read at open.
const PULSE_PROP: &str = "PULSE_PROP";

/// Audio policy role: internet call, in-call volume.
const CALL_ROLE: &str = "media.role=voip";

/// `PULSE_PROP` as found, by `prepare_media_plugins`. Unset: var never written.
static SESSION_PULSE_PROP: OnceLock<String> = OnceLock::new();

/// Label call sound as call while talking.
///
/// Engine sets no role, so sound server used media volume (`x-maemo-match.table`) while volume
/// keys set call volume: keys did nothing. `PULSE_PROP` role applies to connections opened
/// while set; engine opens its one at first sound (call page, if nothing played before).
///
/// Written while threads run, safe here: var exists from before threads, so write replaces
/// string in place, never moves environment; glibc never frees old string.
pub fn label_call_audio(talking: bool) {
    if let Some(session) = SESSION_PULSE_PROP.get() {
        std::env::set_var(PULSE_PROP, pulse_properties(session, talking));
    }
}

/// `PULSE_PROP` value: session's, plus call role after it while talking (later wins).
fn pulse_properties(session: &str, talking: bool) -> String {
    if !talking {
        session.to_owned()
    } else if session.trim().is_empty() {
        CALL_ROLE.to_owned()
    } else {
        format!("{session} {CALL_ROLE}")
    }
}

/// Vars [`prepare_media_plugins`] sets.
fn media_environment(
    var: impl Fn(&str) -> Option<String>,
    log: Option<PathBuf>,
) -> Vec<(&'static str, String)> {
    let ours = var("PIIRIT_GMP_DIR").map_or_else(|| PathBuf::from(BUNDLED_ENCODER), PathBuf::from);
    let mut set = Vec::new();
    if let Some(path) = encoder_path(var("MOZ_GMP_PATH").as_deref(), &ours) {
        set.push(("MOZ_GMP_PATH", path));
    }
    if let Some(log) = log.as_deref().and_then(Path::to_str) {
        set.push(("PIIRIT_CALL_LOG", log.to_owned()));
    }
    set
}

/// Call log: stderr plus own file (launcher drops stderr). Lines prefixed with seconds since
/// call served; file restarted per call.
struct CallLog {
    path: Option<PathBuf>,
    started: Instant,
}

impl CallLog {
    /// New call's log: `path` emptied, or stderr only.
    fn begin(path: Option<PathBuf>) -> Self {
        let path = path.filter(|path| std::fs::write(path, "").is_ok());
        Self {
            path,
            started: Instant::now(),
        }
    }

    /// Log page text as one line.
    fn write(&self, text: &str) {
        let line = log_line(text);
        eprintln!("piirit: call: {line}");
        let Some(path) = &self.path else { return };
        let seconds = self.started.elapsed().as_secs_f64();
        let appended = std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .and_then(|mut file| {
                std::io::Write::write_all(&mut file, format!("{seconds:7.1} {line}\n").as_bytes())
            });
        // Full disk: line still on stderr.
        drop(appended);
    }
}

/// Page log text as one capped line (camera opened, codecs, remote frames).
fn log_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(LOG_LINE_MAX)
        .collect()
}

/// Answer request.
async fn route(
    shared: &Arc<Shared>,
    method: &str,
    target: &str,
    host: &str,
    body: Vec<u8>,
) -> Response {
    if !shared.running.load(Ordering::SeqCst) || host != shared.authority {
        return Response::empty("404 Not Found");
    }
    // Strip query (options) and hash (command).
    let path = target.split(['?', '#']).next().unwrap_or_default();
    if path.starts_with("/call-api") {
        let Some(rest) = path.strip_prefix(&shared.api) else {
            return Response::empty("404 Not Found");
        };
        let text = String::from_utf8_lossy(&body).into_owned();
        return match (method, rest) {
            ("POST", "/start") => place(shared, text).await,
            ("POST", "/accept") => accept_call(shared, text).await,
            ("POST", "/end") => end(shared).await,
            ("POST", "/state") => {
                (shared.report)(Report::Connection(text.trim().to_string()));
                Response::empty("204 No Content")
            }
            ("POST", "/video") => match Video::from_json(&text) {
                Some(video) => {
                    (shared.report)(Report::Video(video));
                    Response::empty("204 No Content")
                }
                None => Response::empty("400 Bad Request"),
            },
            ("POST", "/log") => {
                shared.log.write(&text);
                Response::empty("204 No Content")
            }
            ("GET", "/commands") => commands(shared).await,
            ("GET", "/ice") => ice_servers(shared),
            ("GET", "/avatar") => avatar(shared).await,
            _ => Response::empty("404 Not Found"),
        };
    }
    match (method, path) {
        ("GET", "" | "/" | "/index.html") => {
            Response::new("text/html; charset=utf-8", PAGE.as_bytes().to_vec())
        }
        // Nothing secret (module doc).
        ("GET", "/calls.js") => {
            Response::new("text/javascript; charset=utf-8", BRIDGE.as_bytes().to_vec())
        }
        // Upstream dev setup's webxdc tag, left in build; unused.
        ("GET", "/webxdc.js") => Response::new("text/javascript; charset=utf-8", Vec::new()),
        _ => Response::empty("404 Not Found"),
    }
}

/// Page's offer: place call.
async fn place(shared: &Arc<Shared>, offer: String) -> Response {
    // One call per page, none once this end let go.
    if shared.ending.load(Ordering::SeqCst) || shared.placing.swap(true, Ordering::SeqCst) {
        return Response::empty("409 Conflict");
    }
    let placed: Result<u32, _> = shared
        .rpc
        .call(
            "place_outgoing_call",
            (shared.account_id, shared.chat_id, offer, shared.has_video),
        )
        .await;
    match placed {
        Ok(message_id) => {
            shared.message_id.store(message_id, Ordering::SeqCst);
            // Hung up mid-place: end now.
            if shared.ending.load(Ordering::SeqCst) {
                shared.end().await;
            }
            (shared.report)(Report::Placed(message_id));
            Response::empty("204 No Content")
        }
        Err(err) => {
            (shared.report)(Report::Failed(err.to_string()));
            Response::empty("502 Bad Gateway")
        }
    }
}

/// Page's answer to incoming call: accept.
async fn accept_call(shared: &Arc<Shared>, answer: String) -> Response {
    let message_id = shared.message_id.load(Ordering::SeqCst);
    if message_id == 0 || shared.ending.load(Ordering::SeqCst) {
        return Response::empty("409 Conflict");
    }
    let accepted: Result<serde_json::Value, _> = shared
        .rpc
        .call(
            "accept_incoming_call",
            (shared.account_id, message_id, answer),
        )
        .await;
    match accepted {
        Ok(_) => {
            (shared.report)(Report::Accepted);
            Response::empty("204 No Content")
        }
        Err(err) => {
            (shared.report)(Report::Failed(err.to_string()));
            Response::empty("502 Bad Gateway")
        }
    }
}

/// Page's red button: core told now, not at page removal.
async fn end(shared: &Arc<Shared>) -> Response {
    shared.ending.store(true, Ordering::SeqCst);
    shared.end().await;
    (shared.report)(Report::EndedByPage);
    Response::empty("204 No Content")
}

/// Waiting commands, JSON array. Long poll up to [`COMMAND_WAIT`]; 404 after call: bridge
/// stops.
async fn commands(shared: &Arc<Shared>) -> Response {
    let now = tokio::time::Instant::now();
    let deadline = now.checked_add(COMMAND_WAIT).unwrap_or(now);
    let batch = loop {
        // Listen before look: no lost wakeup.
        let changed = shared.changed.notified();
        let mut changed = std::pin::pin!(changed);
        changed.as_mut().enable();
        if !shared.running.load(Ordering::SeqCst) {
            return Response::empty("404 Not Found");
        }
        let waiting: Vec<String> = shared.queue().drain(..).collect();
        if !waiting.is_empty() {
            break waiting;
        }
        if tokio::time::timeout_at(deadline, changed).await.is_err() {
            break Vec::new();
        }
    };
    Response::new(
        "application/json; charset=utf-8",
        serde_json::to_vec(&batch).unwrap_or_else(|_| b"[]".to_vec()),
    )
}

/// Other end's picture, drawn while no video.
async fn avatar(shared: &Arc<Shared>) -> Response {
    if shared.avatar_path.is_empty() {
        return Response::empty("404 Not Found");
    }
    match tokio::fs::read(&shared.avatar_path).await {
        Ok(bytes) => Response::new(content_type(&shared.avatar_path), bytes),
        Err(_) => Response::empty("404 Not Found"),
    }
}

/// Core's ICE servers JSON; else `[]` (direct reach still works).
fn ice_servers(shared: &Shared) -> Response {
    let servers = if serde_json::from_str::<serde_json::Value>(&shared.ice_servers).is_ok() {
        shared.ice_servers.as_bytes().to_vec()
    } else {
        b"[]".to_vec()
    };
    Response::new("application/json; charset=utf-8", servers)
}

/// Hash command with SDP: padded base64, percent-encoded (`=` separates payload).
pub(crate) fn command(name: &str, sdp: &str) -> String {
    format!("{name}={}", percent_encode(&encode_base64(sdp.as_bytes())))
}

/// Mic off/on: bridge (`calls.js`) presses page's switch.
pub(crate) fn mute_command(on: bool) -> String {
    (if on { "mute" } else { "unmute" }).to_string()
}

/// Bridge (`call_video.js`) commands: camera on/off, facing.
pub(crate) fn camera_command(on: bool) -> String {
    (if on { "camera-on" } else { "camera-off" }).to_string()
}

/// See [`camera_command`].
pub(crate) fn facing_command(front: bool) -> String {
    (if front {
        "facing-user"
    } else {
        "facing-environment"
    })
    .to_string()
}

/// Bridge (`call_video.js`): mic shut and held, or reopened, around moving call sound, which
/// must not happen with mic open (`CallAudio.qml`).
pub(crate) fn microphone_command(on: bool) -> String {
    (if on {
        "microphone-on"
    } else {
        "microphone-off"
    })
    .to_string()
}

/// Bridge: back camera index to open (main lens; engine would take first listed).
pub(crate) fn back_camera_command(place: usize) -> String {
    format!("back-camera-{place}")
}

/// Bridge: stop drawing (background), or resume.
pub(crate) fn shown_command(shown: bool) -> String {
    (if shown { "shown" } else { "hidden" }).to_string()
}

/// Standard base64, padded, for `atob`.
fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
    for chunk in bytes.chunks(3) {
        // Short last chunk read as zero-padded.
        let packed = chunk
            .iter()
            .chain(&[0, 0])
            .take(3)
            .fold(0_u32, |packed, byte| (packed << 8) | u32::from(*byte));
        // n bytes fill n+1 sextets; padding for rest.
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            match ALPHABET.get(((packed >> shift) & 0x3f) as usize) {
                Some(&symbol) if index <= chunk.len() => out.push(char::from(symbol)),
                _ => out.push('='),
            }
        }
    }
    out
}

/// Percent-encode all but `encodeURIComponent` set.
fn percent_encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len().saturating_mul(3) / 2);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            for nibble in [byte >> 4, byte & 0x0f] {
                if let Some(&digit) = HEX.get(usize::from(nibble)) {
                    out.push(char::from(digit));
                }
            }
        }
    }
    out
}

/// Write answer, CSP on each.
async fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Content-Security-Policy: {POLICY}\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n",
        response.status,
        response.content_type,
        response.body.len(),
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(&response.body).await?;
    stream.flush().await
}

#[cfg(test)]
mod tests {
    use super::{
        back_camera_command, camera_command, command, encode_base64, encoder_path, encodes_h264,
        facing_command, keep_pulse_properties, label_call_audio, log_line, media_environment,
        media_plugins, microphone_command, percent_encode, pulse_properties, shown_command,
        CallLog, Video, BRIDGE, BUNDLED_ENCODER, LOG_LINE_MAX, PAGE, PULSE_PROP,
    };
    use crate::webxdc_host::decode_base64;
    use std::path::{Path, PathBuf};

    #[test]
    fn base64_is_what_atob_reads_and_round_trips() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"foob"), "Zm9vYg==");
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(
            decode_base64(&encode_base64(&bytes)).as_deref(),
            Some(&bytes[..])
        );
    }

    #[test]
    fn a_command_survives_the_hash_it_travels_in() {
        // Page: after first `=`, `decodeURIComponent`, `atob`: payload must survive.
        let sdp = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\na=ice-ufrag:+/x\r\n";
        let hash = command("onAnswer", sdp);
        let (name, payload) = hash.split_once('=').expect("a separator");
        assert_eq!(name, "onAnswer");
        assert!(
            !payload.contains(['=', '+', '/', '#', '&']),
            "the payload carries a character the hash would misread: {payload}"
        );
        let decoded = payload
            .replace("%3D", "=")
            .replace("%2B", "+")
            .replace("%2F", "/");
        assert_eq!(
            decode_base64(&decoded).as_deref(),
            Some(sdp.as_bytes()),
            "the payload does not decode back to the SDP"
        );
    }

    #[test]
    fn percent_encoding_leaves_alone_what_encode_uri_component_does() {
        assert_eq!(percent_encode("aZ09-_.!~*'()"), "aZ09-_.!~*'()");
        assert_eq!(percent_encode("a b=c+d/e"), "a%20b%3Dc%2Bd%2Fe");
    }

    #[test]
    fn the_page_asks_for_the_bridge_before_its_own_code_runs() {
        // `calls.js` loads before page module.
        let bridge = PAGE.find("<script src=\"calls.js\"></script>");
        let module = PAGE.find("<script type=\"module\"");
        assert!(
            matches!((bridge, module), (Some(at), Some(code)) if at < code),
            "the vendored page no longer loads calls.js ahead of its own code"
        );
    }

    #[test]
    fn the_bridge_offers_what_the_page_calls_and_holds_no_secret() {
        for name in [
            "startCall",
            "acceptCall",
            "endCall",
            "getIceServers",
            "getAvatar",
        ] {
            assert!(
                BRIDGE.contains(&format!("{name}: function")),
                "the bridge has no {name}"
            );
        }
        // Key from page address; bridge served as written.
        assert!(BRIDGE.contains("key="), "the bridge does not read its key");
        assert!(
            !BRIDGE.contains("__"),
            "the bridge has a placeholder in it, which would be filled with \
             something a page elsewhere could read"
        );
    }

    #[test]
    fn a_video_report_reads_as_the_bridge_writes_it() {
        assert_eq!(
            Video::from_json(r#"{"local":true,"remote":false,"front":true,"failed":false}"#),
            Some(Video {
                local: true,
                remote: false,
                front: true,
                ..Video::default()
            })
        );
        // Engine's error on camera fail.
        assert_eq!(
            Video::from_json(r#"{"failed":true,"error":"NotReadableError: busy"}"#),
            Some(Video {
                failed: true,
                error: "NotReadableError: busy".to_owned(),
                ..Video::default()
            })
        );
        // Which pictures show yet.
        assert_eq!(
            Video::from_json(
                r#"{"local":true,"remote":true,"localPainted":true,"remotePainted":false}"#
            ),
            Some(Video {
                local: true,
                remote: true,
                local_painted: true,
                ..Video::default()
            })
        );
        assert_eq!(
            Video::from_json(r#"{"remotePainted":true}"#),
            Some(Video {
                remote_painted: true,
                ..Video::default()
            })
        );
        // Missing field false; non-object: None.
        assert_eq!(
            Video::from_json(r#"{"remote":true}"#),
            Some(Video {
                remote: true,
                ..Video::default()
            })
        );
        assert_eq!(Video::from_json("[true]"), None);
        assert_eq!(Video::from_json("camera"), None);
    }

    #[test]
    fn a_log_line_is_one_line_and_short() {
        assert_eq!(
            log_line("camera open: front 640x480@30"),
            "camera open: front 640x480@30"
        );
        assert_eq!(log_line("two\nlines\r\nhere"), "two lines  here");
        assert_eq!(log_line(&"x".repeat(1000)).chars().count(), LOG_LINE_MAX);
    }

    #[test]
    fn the_media_plugins_are_logged_with_what_they_do() {
        let dir = std::env::temp_dir().join(format!("piirit-gmp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("libdroid.so"), "").unwrap();
        std::fs::write(
            dir.join("droid.info"),
            "Name: gmp-droid\nAPIs: decode-video[h264], encode-video[h264]\n",
        )
        .unwrap();
        let shown = dir.to_string_lossy().into_owned();
        let line = media_plugins(Some(&format!("{shown}:/piirit-no-such-dir")));
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(
            line,
            format!(
                "media plugins: {shown} decode-video[h264], encode-video[h264]; \
                 /piirit-no-such-dir (no APIs found)"
            )
        );
        assert_eq!(
            media_plugins(None),
            "media plugins: MOZ_GMP_PATH is not set"
        );
        assert_eq!(
            media_plugins(Some("")),
            "media plugins: MOZ_GMP_PATH is not set"
        );
    }

    /// Plugin dir with `<name>.info` of `apis`.
    fn plugin(test: &str, name: &str, apis: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("piirit-plugins-{test}-{}", std::process::id()))
            .join(format!("gmp-{name}"))
            .join("1.0");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{name}.info")),
            format!("Name: {name}\nDescription: a plugin\nVersion: 1.0\nAPIs: {apis}\n"),
        )
        .unwrap();
        dir
    }

    fn text(dir: &Path) -> String {
        dir.to_string_lossy().into_owned()
    }

    #[test]
    fn an_h264_encoder_is_read_from_its_apis_as_gecko_reads_them() {
        assert!(encodes_h264("encode-video[h264]"));
        assert!(encodes_h264(
            "decode-video[h264:vp8], encode-video[vp8:h264]"
        ));
        assert!(!encodes_h264("decode-video[h264]"));
        assert!(!encodes_h264("encode-video[vp8]"));
        assert!(!encodes_h264("encode-video[h2645]"));
        assert!(!encodes_h264("encode-video"));
        // Split at ", " only; tags end at first "]".
        assert!(!encodes_h264("encode-video[vp8],encode-video[h264]"));
    }

    #[test]
    fn gecko_is_pointed_at_the_bundled_encoder_unless_the_phone_has_one() {
        let ours = plugin("path", "piirit", "encode-video[h264]");
        let decoder = text(&plugin("path", "droid", "decode-video[h264:vp8]"));
        let vp8 = text(&plugin("path", "vp8", "encode-video[vp8]"));
        let hardware = text(&plugin(
            "path",
            "hardware",
            "decode-video[h264], encode-video[vp8:h264]",
        ));
        let missing = ours.join("not-installed");
        let our = text(&ours);

        let paths = [
            encoder_path(None, &ours),
            encoder_path(Some(""), &ours),
            // gmp-droid: phone decodes H.264, can't encode.
            encoder_path(Some(&decoder), &ours),
            encoder_path(Some(&format!(":{vp8}::{decoder}:")), &ours),
            // gmp-droid, phone encodes too.
            encoder_path(Some(&hardware), &ours),
            encoder_path(Some(&format!("{decoder}:{hardware}")), &ours),
            // Already there.
            encoder_path(Some(&format!("{decoder}:{our}/")), &ours),
            // Not installed.
            encoder_path(None, &missing),
        ];
        std::fs::remove_dir_all(ours.parent().unwrap().parent().unwrap()).unwrap();

        assert_eq!(
            paths,
            [
                Some(our.clone()),
                Some(our.clone()),
                Some(format!("{decoder}:{our}")),
                Some(format!("{vp8}:{decoder}:{our}")),
                None,
                None,
                None,
                None,
            ]
        );
    }

    #[test]
    fn the_environment_names_the_encoder_and_the_call_log() {
        let ours = plugin("env", "piirit", "encode-video[h264]");
        let our = text(&ours);
        let hardware = text(&plugin("env", "hardware", "encode-video[h264]"));
        let log = PathBuf::from("/home/me/.cache/piirit/piirit/call.log");
        let vars = |moz: Option<&str>| {
            let moz = moz.map(str::to_owned);
            let our = our.clone();
            move |name: &str| match name {
                "PIIRIT_GMP_DIR" => Some(our.clone()),
                "MOZ_GMP_PATH" => moz.clone(),
                _ => None,
            }
        };

        let fresh = media_environment(vars(None), Some(log.clone()));
        let with_hardware = media_environment(vars(Some(&hardware)), None);
        let installed = media_environment(|_| None, Some(log.clone()));
        std::fs::remove_dir_all(ours.parent().unwrap().parent().unwrap()).unwrap();

        assert_eq!(
            fresh,
            [
                ("MOZ_GMP_PATH", our.clone()),
                ("PIIRIT_CALL_LOG", text(&log)),
            ]
        );
        assert!(with_hardware.is_empty(), "{with_hardware:?}");
        // No PIIRIT_GMP_DIR: package path, absent in tests.
        assert!(!Path::new(BUNDLED_ENCODER).exists());
        assert_eq!(installed, [("PIIRIT_CALL_LOG", text(&log))]);
    }

    #[test]
    fn a_call_says_its_role_after_the_sessions_properties() {
        assert_eq!(pulse_properties("", false), "");
        assert_eq!(pulse_properties("", true), "media.role=voip");
        assert_eq!(pulse_properties("  ", true), "media.role=voip");
        let session = "application.icon_name=piirit media.role=music";
        assert_eq!(pulse_properties(session, false), session);
        assert_eq!(
            pulse_properties(session, true),
            "application.icon_name=piirit media.role=music media.role=voip"
        );
    }

    #[test]
    fn a_call_labels_the_sound_while_it_is_on_and_puts_it_back() {
        // Only test writing env.
        let read = || std::env::var(PULSE_PROP);
        std::env::remove_var(PULSE_PROP);
        label_call_audio(true);
        assert!(read().is_err(), "{:?}", read());
        keep_pulse_properties();
        assert_eq!(read().as_deref(), Ok(""));
        label_call_audio(true);
        assert_eq!(read().as_deref(), Ok("media.role=voip"));
        label_call_audio(false);
        assert_eq!(read().as_deref(), Ok(""));
    }

    #[test]
    fn a_call_log_holds_one_call_line_by_line() {
        let path = std::env::temp_dir().join(format!("piirit-call-log-{}", std::process::id()));
        std::fs::write(&path, "the call before\n").unwrap();

        let log = CallLog::begin(Some(path.clone()));
        log.write("camera open: front");
        log.write("video stats:\nsent");
        let kept = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        let lines: Vec<&str> = kept.lines().collect();
        assert_eq!(lines.len(), 2, "{kept:?}");
        // Seconds since call began.
        assert!(lines[0].starts_with("    0.0 "), "{kept:?}");
        assert!(lines[0].ends_with(" camera open: front"), "{kept:?}");
        assert!(lines[1].ends_with(" video stats: sent"), "{kept:?}");

        // No file: stderr only.
        CallLog::begin(None).write("camera open: front");
        let missing = std::env::temp_dir()
            .join("piirit-no-such-dir")
            .join("call.log");
        CallLog::begin(Some(missing.clone())).write("camera open: front");
        assert!(!missing.exists());
    }

    #[test]
    fn the_camera_and_microphone_commands_are_the_ones_the_bridge_takes() {
        // `take` names these; unknown ignored.
        for (made, name) in [
            (camera_command(true), "camera-on"),
            (camera_command(false), "camera-off"),
            (facing_command(true), "facing-user"),
            (facing_command(false), "facing-environment"),
            (shown_command(true), "shown"),
            (shown_command(false), "hidden"),
            (microphone_command(true), "microphone-on"),
            (microphone_command(false), "microphone-off"),
        ] {
            assert_eq!(made, name);
            assert!(
                BRIDGE.contains(&format!("case \"{name}\":")),
                "the bridge does not take {name}"
            );
        }
        assert_eq!(back_camera_command(2), "back-camera-2");
        assert!(
            BRIDGE.contains(r"/^back-camera-(\d+)$/"),
            "the bridge does not take back-camera-N"
        );
    }

    #[test]
    fn the_page_still_has_what_the_camera_half_reads_off_it() {
        // Camera half matches these page labels.
        for needle in [
            "Start camera",
            "Stop camera",
            "noOutgoingVideoInitially",
            "mutedState",
        ] {
            assert!(
                PAGE.contains(needle),
                "the vendored page no longer has {needle:?}"
            );
        }
    }

    #[test]
    fn the_camera_half_is_served_ahead_of_the_bridge_that_uses_it() {
        let defined = BRIDGE.find("var piiritVideo =");
        let used = BRIDGE.find("piiritVideo.attach(");
        assert!(
            matches!((defined, used), (Some(at), Some(use_at)) if at < use_at),
            "the bridge uses the camera's half before it is defined"
        );
    }
}
