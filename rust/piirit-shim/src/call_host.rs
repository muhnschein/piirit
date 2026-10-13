//! The loopback host one call's page is served from while the call is up.
//!
//! A call's media is the browser engine's: Gecko has the peer connection,
//! the codecs and the echo cancellation, and upstream's calls-webapp is
//! the page that drives them (`vendor/calls-webapp/`). What the page needs
//! from outside itself is the core -- to place the call, to accept it, to
//! end it -- and it asks for that through `window.calls`, which is ours
//! to provide. So the page is served from here, and its `calls.js` is a
//! bridge whose five functions are requests back to this host (`calls.js`
//! in this directory), the arrangement `webxdc_host.rs` is for an app.
//!
//! The other direction -- the other end's answer, arriving as a core
//! event -- is a hash command the page reads (`#onAnswer=...`). The
//! [`Call`](crate::calls::Call) object hears the event on the Qt thread
//! and hands the command here with [`Host::command`]; the bridge collects
//! it with a long poll and sets it on the page. The app's own microphone
//! switch goes the same way ([`mute_command`]), and the bridge has the
//! page press its switch rather than setting anything.
//!
//! What keeps it from being a hole in the phone is what keeps the webxdc
//! host from being one: 127.0.0.1 on a port the kernel picks, a `Host:`
//! that must be the address the page was given, every route that reaches
//! the core behind an unguessable token, and nothing served once the call
//! is over. The page is compiled in, so nothing on disk can be swapped
//! for it.
//!
//! And one thing more: the token is only in the address the app hands its
//! own `WebView`. The bridge reads it from there, and the bridge the host
//! serves carries none. A page anywhere else can load `calls.js` into
//! itself with a script tag -- a browser allows that across origins, and
//! the `Host:` it sends is the right one -- and learns nothing from it
//! with which to answer a ringing call or end one.
//!
//! Its policy differs in one place that matters: the page's whole job is
//! to reach the other end over the network, and it does so through the
//! peer connection, which a content-security-policy does not govern. What
//! the policy does govern -- fetches, scripts, frames -- stays on this
//! origin, so the page cannot phone anywhere by any other route.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use deltachat_jsonrpc::RpcClient;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::json;
use crate::webxdc_host::{content_type, read_body, read_head, token};

/// The page itself: upstream's release build, byte for byte. Compiled in
/// for the reason the webxdc bridge is: it is served rather than loaded,
/// and a file the package could lose is a call that fails on a phone and
/// nowhere else. `scripts/fetch-calls-webapp.sh` pins what it is.
const PAGE: &str = include_str!("../../../vendor/calls-webapp/index.html");

/// `window.calls`, as JavaScript: the camera's half (`call_video.js`)
/// ahead of the bridge, which hands it what it needs.
const BRIDGE: &str = concat!(include_str!("call_video.js"), include_str!("calls.js"));

/// What the page may do from inside the `WebView`. Its own script and
/// style are inline, and it draws the other end's picture from here;
/// every fetch it could make stays on this origin. The peer connection is
/// not a fetch, and is not held back by any of this -- which is the point
/// of the page.
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

/// How long a request for commands waits for one before answering with
/// none. The bridge asks again at once, so this only decides how often a
/// quiet call costs a request.
const COMMAND_WAIT: Duration = Duration::from_secs(20);

/// Every host serving a call, for [`end_all`]. Weak, so a host that has
/// gone -- and whose call has been ended -- is not kept here.
static HOSTS: Mutex<Vec<Weak<Shared>>> = Mutex::new(Vec::new());

/// How many messages the core has said it is done sending, one way or
/// the other; see [`note_sent`].
static SENT: AtomicU64 = AtomicU64::new(0);

/// How often [`end_all`] looks for the message ending a call to have gone.
const SENT_POLL: Duration = Duration::from_millis(50);

/// What the host was asked to serve.
pub(crate) struct Setup {
    /// The account the call belongs to.
    pub(crate) account_id: u32,
    /// The chat it is in: what an outgoing call is placed into.
    pub(crate) chat_id: u32,
    /// The call's message. Known from the start for a call that came in;
    /// 0 for one this end is placing, until the core has placed it.
    pub(crate) message_id: u32,
    /// Whether an outgoing call is placed as a video call.
    pub(crate) has_video: bool,
    /// The core's ICE servers, as the JSON string it hands them over in.
    pub(crate) ice_servers: String,
    /// The other end's picture, a file the core keeps; empty for none.
    pub(crate) avatar_path: String,
}

/// What happened, as the host reports it back to the call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Report {
    /// The core placed the call, as this message.
    Placed(u32),
    /// The core has the page's answer to the call that came in.
    Accepted,
    /// The peer connection says how it is doing: ICE's own words,
    /// `checking`, `connected`, `completed`, `disconnected`, `failed`,
    /// `closed`.
    Connection(String),
    /// How the pictures stand: the camera live here, the other end's
    /// picture drawn, which way the camera faces, and whether the last
    /// camera asked for would not open.
    Video(Video),
    /// The page asked for the call to end: its red button.
    EndedByPage,
    /// The core refused something, in its own words.
    Failed(String),
}

/// How a call's pictures stand, as the bridge reports them on `/video`.
// Flags the bridge reports side by side, each read on its own.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Video {
    /// The camera is open and sending.
    pub(crate) local: bool,
    /// The other end's picture is being drawn.
    pub(crate) remote: bool,
    /// The camera's picture has painted a frame since it was last given
    /// one -- shown, not black -- or has been given up on.
    pub(crate) local_painted: bool,
    /// The same, of the other end's picture.
    pub(crate) remote_painted: bool,
    /// The camera faces the reader.
    pub(crate) front: bool,
    /// The last camera asked for would not open.
    pub(crate) failed: bool,
    /// What the engine said when it would not: the error's name and
    /// message, empty when it said nothing.
    pub(crate) error: String,
}

impl Video {
    /// Read the bridge's report: a JSON object, a missing field false.
    /// None for anything that is not an object.
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

/// Where reports go: to the call on the Qt thread, through a queued
/// callback, as `webxdc_host::HandedOver` goes to its page.
pub(crate) type Reporter = Arc<dyn Fn(Report) + Send + Sync>;

/// Everything a connection needs, shared by every one of them.
struct Shared {
    rpc: Arc<RpcClient>,
    account_id: u32,
    chat_id: u32,
    has_video: bool,
    ice_servers: String,
    avatar_path: String,
    /// The call's message, once there is one. 0 until then.
    message_id: AtomicU32,
    /// The unguessable part of the routes that reach the core, which the
    /// page's own address carries and nothing the host serves does.
    key: String,
    /// `/call-api/<key>`: the routes that reach the core.
    api: String,
    /// The `Host:` a request must carry.
    authority: String,
    /// Cleared when the host is dropped: nothing is served after it.
    running: AtomicBool,
    /// Set once this end is done with the call -- the page's red button,
    /// or the app hanging up. A place still on its way to the core is
    /// ended as soon as it lands, rather than left ringing at the other
    /// end for a call nobody here is holding.
    ending: AtomicBool,
    /// Set once `end_call` has gone out, so it goes out once -- or once
    /// the core is known not to need it; see [`Host::settle`].
    ended: AtomicBool,
    /// Set once `end_call` has gone out from here: the core has a message
    /// to send the other end.
    told: AtomicBool,
    /// Held while `end_call` is on its way, so that whoever asks second
    /// -- the app closing just after a hang-up -- waits for the answer.
    ending_call: tokio::sync::Mutex<()>,
    /// Set once the page has asked for the call to be placed. The page
    /// asks once; a second ask would be a second call.
    placing: AtomicBool,
    /// Hash commands for the page, waiting to be collected.
    commands: Mutex<VecDeque<String>>,
    /// Woken when a command is queued and when the host goes.
    changed: Notify,
    report: Reporter,
    /// What the page asked to have logged.
    log: CallLog,
}

impl Shared {
    fn queue(&self) -> std::sync::MutexGuard<'_, VecDeque<String>> {
        self.commands
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Tell the core the call is over, once, if there is a call to end.
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

/// A served call: where its page is, and the task serving it.
///
/// Dropping it stops the host and, when this end is the one leaving,
/// tells the core so.
pub(crate) struct Host {
    /// Where the page is, without its query or its command.
    page: String,
    shared: Arc<Shared>,
    task: JoinHandle<()>,
    /// Where ending the call runs once the call has gone: a drop is not
    /// async, and happens on the Qt thread.
    runtime: Handle,
}

impl Host {
    /// Where to point the `WebView`: the page, with the page's own
    /// `options` and the key in its query, and `command` as its hash.
    pub(crate) fn url(&self, options: &str, command: &str) -> String {
        format!("{}?{options}&key={}#{command}", self.page, self.shared.key)
    }

    /// Hand the page a command, which the bridge collects: a hash
    /// command, or one of the bridge's own.
    pub(crate) fn command(&self, command: String) {
        self.shared.queue().push_back(command);
        self.shared.changed.notify_waiters();
    }

    /// The call is over at the other end, and the core knows: nothing
    /// is to be told to it when this host goes.
    pub(crate) fn settle(&self) {
        self.shared.ended.store(true, Ordering::SeqCst);
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::SeqCst);
        self.shared.ending.store(true, Ordering::SeqCst);
        self.task.abort();
        // A request waiting for a command answers now rather than holding
        // its connection open for a page that has gone.
        self.shared.changed.notify_waiters();
        // Whatever ends the call here, the core hears it once. A call the
        // other end already ended is one the core has ended too, and
        // `end_call` on it is a no-op there.
        let shared = Arc::clone(&self.shared);
        drop(self.runtime.spawn(async move { shared.end().await }));
    }
}

/// Serve one call's page on the loopback interface until the [`Host`] is
/// dropped.
///
/// # Errors
///
/// Fails when the loopback port cannot be bound.
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

/// End every call still hosted here, and give the core the moment it
/// needs to tell the other end: for the app's last moment, before the
/// core is stopped. Bounded by the caller.
///
/// The reader closing the app mid-call takes the call with it, but the
/// core is stopped before anything on the Qt side lets go of its host --
/// so this is where the core hears of it, rather than nowhere. A call
/// that is only ringing here has no host and is left alone: the app
/// closing is not a decline, and another device may yet answer it.
///
/// `end_call` only queues the message that tells the other end, and
/// sending it is the core's own work, which stopping the core cuts off.
/// So once a call has been ended this waits for the core to say it has
/// sent a message -- without it the other end rings on, or talks to
/// nobody, until its own timeout.
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
        // A call still being placed is ended as soon as it lands.
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

/// One core event, looked at for whether the core is done sending a
/// message: sent, delivered, or given up on. What [`end_all`] waits for.
pub(crate) fn note_sent(kind: &str) {
    if matches!(kind, "SmtpMessageSent" | "MsgDelivered" | "MsgFailed") {
        SENT.fetch_add(1, Ordering::SeqCst);
    }
}

/// A page load is a handful of connections at once, and one of them is
/// a long poll that holds on; each gets a task of its own.
async fn accept(listener: TcpListener, shared: Arc<Shared>) {
    while shared.running.load(Ordering::SeqCst) {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            // A connection that fails is one the page has given up on, and
            // there is nobody left on it to answer.
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

    /// A refusal, carrying its own reason; see `webxdc_host`.
    fn empty(status: &'static str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: status.as_bytes().to_vec(),
        }
    }
}

/// Read one request, answer it, and close.
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

/// The longest line the page may put in the app's log.
const LOG_LINE_MAX: usize = 300;

/// The file the last call's log is kept in, among the app's cache files.
const LOG_FILE: &str = "call.log";

/// What the engine's media plugins say they do, for the log: the
/// directories in `MOZ_GMP_PATH`, each with the `APIs:` line of its
/// `.info` file. Gecko encodes H264 only through such a plugin --
/// Sailfish's is gmp-droid, the phone's hardware encoder -- and an iPhone
/// at the other end offers no other codec, so a call that sends no
/// picture there starts here.
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

/// The `APIs:` line of the `.info` file in a media plugin's directory:
/// `decode-video[h264:vp8], encode-video[h264]` for gmp-droid on a phone
/// whose hardware does both.
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

/// Whether an `APIs:` line offers an H.264 encoder, read as Gecko reads
/// it (`GMPParent::ReadGMPInfoFile`): capabilities apart at ", ", each a
/// name and its tags in brackets, apart at ":".
fn encodes_h264(apis: &str) -> bool {
    apis.split(", ").any(|capability| {
        capability
            .strip_prefix("encode-video[")
            .and_then(|tags| tags.strip_suffix(']'))
            .is_some_and(|tags| tags.split(':').any(|tag| tag == "h264"))
    })
}

/// Where the package installs the app's own H.264 encoder: a media
/// plugin built from the binary's x264 (`piirit-video/build.rs`).
const BUNDLED_ENCODER: &str = "/usr/share/harbour-piirit/lib/gmp-piirit/1.0";

/// What `MOZ_GMP_PATH` should be for a call to send H.264, given what it
/// is: the same, and `ours` after it -- unless a plugin it already names
/// encodes H.264. That is gmp-droid, which sets the variable for the
/// whole session when it is installed, and the phone's hardware encoder
/// costs less than any software one -- or `ours` itself, already there.
/// `None` leaves the variable alone, as it does when `ours` is not
/// installed.
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

/// Before the app starts a thread, and so before the browser engine
/// does: point Gecko at an H.264 encoder (see `encoder_path`), and tell
/// the encoder where the call's log is. Gecko starts the plugin as a
/// process of its own, which inherits both. `PIIRIT_GMP_DIR` names
/// another encoder than the one the package installs, for trying a
/// build on a phone.
pub fn prepare_media_plugins() {
    let log = crate::qr::cache_file(LOG_FILE).ok();
    for (name, value) in media_environment(|name| std::env::var(name).ok(), log) {
        std::env::set_var(name, value);
    }
}

/// The variables [`prepare_media_plugins`] sets, from the ones it reads
/// through `var` and the call log's path.
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

/// A call's log: each line on stderr, where a phone's log has it when
/// the app is started from a terminal, and in a file of its own -- the
/// app's stderr goes nowhere when the launcher starts it. One call's
/// lines, each with the seconds since the call was served: the file is
/// begun afresh with every call, so it holds the call that has just
/// gone wrong, and never grows.
struct CallLog {
    path: Option<PathBuf>,
    started: Instant,
}

impl CallLog {
    /// A log for a call starting now, in the file at `path` -- emptied --
    /// or on stderr alone when there is nowhere for the file.
    fn begin(path: Option<PathBuf>) -> Self {
        let path = path.filter(|path| std::fs::write(path, "").is_ok());
        Self {
            path,
            started: Instant::now(),
        }
    }

    /// Log what the page sent, as one line.
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
        // A full disk takes nothing from the call: the line is on stderr.
        drop(appended);
    }
}

/// What the page asked to have logged, as one line of the log: what only
/// a phone shows about a call -- which camera opened, which codecs were
/// agreed, whether the other end's picture has frames -- read where the
/// app's log is. Kept to one line, and to a length, whatever the page
/// sends.
fn log_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(LOG_LINE_MAX)
        .collect()
}

/// Pick the answer.
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
    // The page is loaded with its options in the query and its command
    // in the hash, and neither reaches here as part of the path.
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
        // The same for every call, and for anybody: nothing in it is a
        // secret. See the top of this file.
        ("GET", "/calls.js") => {
            Response::new("text/javascript; charset=utf-8", BRIDGE.as_bytes().to_vec())
        }
        // The page asks for this too: upstream's development setup runs
        // it as a webxdc app, and the tag stays in the build. Nothing in
        // the page calls it outside that setup.
        ("GET", "/webxdc.js") => Response::new("text/javascript; charset=utf-8", Vec::new()),
        _ => Response::empty("404 Not Found"),
    }
}

/// The page has its offer: place the call.
async fn place(shared: &Arc<Shared>, offer: String) -> Response {
    // One call per page, and none once this end has let go of it.
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
            // Hung up while the core was placing it: the other end is
            // ringing for nobody, so it is told at once.
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

/// The page has its answer to the call that came in: accept it.
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

/// The page's red button. The core is told here rather than when the
/// page goes, so the other end stops ringing even if the app is slow to
/// take the page away.
async fn end(shared: &Arc<Shared>) -> Response {
    shared.ending.store(true, Ordering::SeqCst);
    shared.end().await;
    (shared.report)(Report::EndedByPage);
    Response::empty("204 No Content")
}

/// Whatever commands are waiting, as a JSON array of hash strings. A long
/// poll: with none waiting this holds on until one is queued or
/// [`COMMAND_WAIT`] has passed. A 404 once the call is over, which is the
/// bridge's cue to stop asking.
async fn commands(shared: &Arc<Shared>) -> Response {
    let now = tokio::time::Instant::now();
    let deadline = now.checked_add(COMMAND_WAIT).unwrap_or(now);
    let batch = loop {
        // Listening before looking, so a command queued between the two
        // still wakes this.
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

/// The other end's picture, which the page draws while there is no video.
async fn avatar(shared: &Arc<Shared>) -> Response {
    if shared.avatar_path.is_empty() {
        return Response::empty("404 Not Found");
    }
    match tokio::fs::read(&shared.avatar_path).await {
        Ok(bytes) => Response::new(content_type(&shared.avatar_path), bytes),
        Err(_) => Response::empty("404 Not Found"),
    }
}

/// The core's ICE servers, as the page parses them. The core hands them
/// over as a JSON string; one that is not JSON is no servers, which the
/// page can still call without -- on a network where the two ends reach
/// each other directly.
fn ice_servers(shared: &Shared) -> Response {
    let servers = if serde_json::from_str::<serde_json::Value>(&shared.ice_servers).is_ok() {
        shared.ice_servers.as_bytes().to_vec()
    } else {
        b"[]".to_vec()
    };
    Response::new("application/json; charset=utf-8", servers)
}

/// A hash command with an SDP in it, as the page reads one: the payload
/// in standard base64, padding and all, then percent-encoded, since `=`
/// is also what separates the command from its payload.
pub(crate) fn command(name: &str, sdp: &str) -> String {
    format!("{name}={}", percent_encode(&encode_base64(sdp.as_bytes())))
}

/// The command that has the page switch its microphone off, or on.
/// Not a hash command: the bridge (`calls.js`) takes these two for
/// itself, and has the page press its own switch.
pub(crate) fn mute_command(on: bool) -> String {
    (if on { "mute" } else { "unmute" }).to_string()
}

/// The commands that have the bridge (`call_video.js`) switch the camera
/// on or off, and turn it to face the reader or away.
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

/// The commands that have the bridge (`call_video.js`) shut the
/// microphone and keep it shut, or open it again: around moving the
/// call's sound, which must not happen with it open (CallAudio.qml).
pub(crate) fn microphone_command(on: bool) -> String {
    (if on {
        "microphone-on"
    } else {
        "microphone-off"
    })
    .to_string()
}

/// The command that tells the bridge which of the phone's back cameras
/// to open, as its place among them: the main lens, where the engine
/// would take the first it lists.
pub(crate) fn back_camera_command(place: usize) -> String {
    format!("back-camera-{place}")
}

/// The command that has the bridge stop drawing the pictures, the app
/// having gone to the background, or draw them again.
pub(crate) fn shown_command(shown: bool) -> String {
    (if shown { "shown" } else { "hidden" }).to_string()
}

/// Standard base64 with padding: what the page's `atob` reads.
fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
    for chunk in bytes.chunks(3) {
        // A short last chunk is read as if zeros filled it out.
        let packed = chunk
            .iter()
            .chain(&[0, 0])
            .take(3)
            .fold(0_u32, |packed, byte| (packed << 8) | u32::from(*byte));
        // A chunk of n bytes fills n + 1 sextets; padding stands for the rest.
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            match ALPHABET.get(((packed >> shift) & 0x3f) as usize) {
                Some(&symbol) if index <= chunk.len() => out.push(char::from(symbol)),
                _ => out.push('='),
            }
        }
    }
    out
}

/// Percent-encode everything but what `encodeURIComponent` leaves alone.
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

/// Write the answer out, with the policy on every one of them.
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
        facing_command, log_line, media_environment, media_plugins, microphone_command,
        percent_encode, shown_command, CallLog, Video, BRIDGE, BUNDLED_ENCODER, LOG_LINE_MAX, PAGE,
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
        // The page takes everything after the first `=` and
        // `decodeURIComponent`s it before `atob`: so nothing in the
        // payload may be read as the separator, or as the end of the
        // hash, or be mangled by the decoding.
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
        // What the whole arrangement stands on: the page loads a classic
        // `calls.js` from beside itself, ahead of its module, so that
        // `window.calls` is there by the time the page reaches for it.
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
        // The key comes from the page's own address, and nothing is
        // filled into the bridge: it is served as it is written.
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
        // A camera that would not open, with what the engine said.
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
        // A field left out is false, and anything but an object nothing.
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

    /// A media plugin's directory, `<name>.info` in it saying `apis`,
    /// under a directory of the test's own.
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
        // Gecko splits capabilities at ", " and nothing else; this is one
        // capability whose tags end at the first "]".
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
            // gmp-droid where the phone decodes H.264 but cannot encode it.
            encoder_path(Some(&decoder), &ours),
            encoder_path(Some(&format!(":{vp8}::{decoder}:")), &ours),
            // gmp-droid where it can: the hardware does it for less.
            encoder_path(Some(&hardware), &ours),
            encoder_path(Some(&format!("{decoder}:{hardware}")), &ours),
            // Already there, however it was written.
            encoder_path(Some(&format!("{decoder}:{our}/")), &ours),
            // Not installed: nothing to point at.
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
        // Without PIIRIT_GMP_DIR, the package's own path -- not
        // installed where the tests run, so left out.
        assert!(!Path::new(BUNDLED_ENCODER).exists());
        assert_eq!(installed, [("PIIRIT_CALL_LOG", text(&log))]);
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
        // Seconds since the call began, then the line.
        assert!(lines[0].starts_with("    0.0 "), "{kept:?}");
        assert!(lines[0].ends_with(" camera open: front"), "{kept:?}");
        assert!(lines[1].ends_with(" video stats: sent"), "{kept:?}");

        // Nowhere for the file: stderr alone, and nothing breaks.
        CallLog::begin(None).write("camera open: front");
        let missing = std::env::temp_dir()
            .join("piirit-no-such-dir")
            .join("call.log");
        CallLog::begin(Some(missing.clone())).write("camera open: front");
        assert!(!missing.exists());
    }

    #[test]
    fn the_camera_and_microphone_commands_are_the_ones_the_bridge_takes() {
        // call_video.js's `take` names these; a command it does not know
        // would be set on the page as a hash, and do nothing.
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
        // call_video.js presses the page's camera switch by its labels,
        // reads its option the way it does, and listens on its channel
        // for the other end's camera: a new release that renamed any of
        // them would leave the camera switched one way and the page
        // telling the other end the other.
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
