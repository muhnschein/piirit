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
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use deltachat_jsonrpc::RpcClient;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::webxdc_host::{content_type, read_body, read_head, token};

/// The page itself: upstream's release build, byte for byte. Compiled in
/// for the reason the webxdc bridge is: it is served rather than loaded,
/// and a file the package could lose is a call that fails on a phone and
/// nowhere else. `scripts/fetch-calls-webapp.sh` pins what it is.
const PAGE: &str = include_str!("../../../vendor/calls-webapp/index.html");

/// `window.calls`, as JavaScript.
const BRIDGE: &str = include_str!("calls.js");

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
    /// The page asked for the call to end: its red button.
    EndedByPage,
    /// The core refused something, in its own words.
    Failed(String),
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
    /// Set once `end_call` has gone out, so it goes out once.
    ended: AtomicBool,
    /// Set once the page has asked for the call to be placed. The page
    /// asks once; a second ask would be a second call.
    placing: AtomicBool,
    /// Hash commands for the page, waiting to be collected.
    commands: Mutex<VecDeque<String>>,
    /// Woken when a command is queued and when the host goes.
    changed: Notify,
    report: Reporter,
}

impl Shared {
    fn queue(&self) -> std::sync::MutexGuard<'_, VecDeque<String>> {
        self.commands
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Tell the core the call is over, once, if there is a call to end.
    async fn end(&self) {
        let message_id = self.message_id.load(Ordering::SeqCst);
        if message_id == 0 || self.ended.swap(true, Ordering::SeqCst) {
            return;
        }
        let _: Result<serde_json::Value, _> = self
            .rpc
            .call("end_call", (self.account_id, message_id))
            .await;
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
        placing: AtomicBool::new(false),
        commands: Mutex::new(VecDeque::new()),
        changed: Notify::new(),
        report,
    });
    let task = tokio::spawn(accept(listener, Arc::clone(&shared)));
    Ok(Host {
        page,
        shared,
        task,
        runtime: Handle::current(),
    })
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
            let _ = answer(stream, shared).await;
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
    let deadline = tokio::time::Instant::now() + COMMAND_WAIT;
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

/// Standard base64 with padding: what the page's `atob` reads.
fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let packed = chunk
            .iter()
            .enumerate()
            .fold(0_u32, |packed, (index, byte)| {
                packed | (u32::from(*byte) << (16 - 8 * index))
            });
        for index in 0..4 {
            if index <= chunk.len() {
                let sextet = (packed >> (18 - 6 * index)) & 0x3f;
                out.push(char::from(ALPHABET[sextet as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Percent-encode everything but what `encodeURIComponent` leaves alone.
fn percent_encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len() * 3 / 2);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
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
    use super::{command, encode_base64, percent_encode, BRIDGE, PAGE};
    use crate::webxdc_host::decode_base64;

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
}
