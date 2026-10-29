//! Loopback host for one webxdc app, while open.
//!
//! `WebView` needs the zip as a website: files served from core (`get_webxdc_blob`), never
//! unpacked here. Way back is HTTP too: `webxdc.js` POSTs updates and long-polls until core pokes
//! (`Host::poke`); needs only the URL, quiet app costs a request per [`UPDATE_WAIT`]. Gecko frame
//! script alternative is chrome-privileged, untestable here.
//!
//! Served from root, as other clients: bundler-default apps ask `/assets/index-1a2b.js`; a
//! prefix blanks half the store.
//!
//! Guards:
//! - 127.0.0.1 on kernel port: unreachable off device;
//! - `Host:` must match: no DNS trick;
//! - chat API under unguessable `/webxdc-api/<token>/`: port finder gets app files, not chat;
//! - exists only while app open;
//! - CSP: this origin only, no network.
//!
//! App core lets online (maps integration) gets OSM tiles via core, behind token: [`TILES`].

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use deltachat_jsonrpc::RpcClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

/// API JS, compiled in: packaged file could go missing on phone only.
const BRIDGE: &str = include_str!("webxdc.js");

/// `'self'`: this host only. `unsafe-inline`/`unsafe-eval` for app scripts: origin can't reach out.
const POLICY: &str = "default-src 'self'; \
     script-src 'self' 'unsafe-inline' 'unsafe-eval' blob:; \
     style-src 'self' 'unsafe-inline' blob:; \
     img-src 'self' data: blob:; \
     media-src 'self' data: blob:; \
     font-src 'self' data: blob:; \
     connect-src 'self' data: blob:; \
     form-action 'none'; \
     frame-ancestors 'none'";

/// Tile server maps app asks; rewritten in its scripts to `<api>/tile/`, fetched by core
/// (`get_http_response`: its cache, user agent, proxy). OSM refuses browser engine's own asks.
const TILES: &str = "https://tile.openstreetmap.org/";

/// Ms before re-asking after failed update request.
const RETRY_INTERVAL: u32 = 300;

/// Max update wait before empty answer: quiet app's ask rate. As `call_host.rs` command wait.
const UPDATE_WAIT: Duration = Duration::from_secs(20);

/// Max request head; webxdc's own are short.
const MAX_HEAD: usize = 16 * 1024;

/// Max `POST`ed update. Core soft limit 100 KiB; larger body unread.
const MAX_BODY: usize = 512 * 1024;

/// Max refused body drained before answering: closing on a writing client resets it, app
/// sees unreachable host.
const MAX_DRAIN: usize = 256 * 1024 * 1024;

/// Served app and what it is told.
pub(crate) struct Instance {
    /// Account of message.
    pub(crate) account_id: u32,
    /// Message webxdc arrived as; status-update key.
    pub(crate) message_id: u32,
    /// `window.webxdc.selfAddr`: core's answer, not account address.
    pub(crate) self_addr: String,
    /// `window.webxdc.selfName`: profile name.
    pub(crate) self_name: String,
    /// `window.webxdc.sendUpdateInterval`, ms.
    pub(crate) send_update_interval: u64,
    /// `window.webxdc.sendUpdateMaxSize`, bytes.
    pub(crate) send_update_max_size: u64,
    /// Core lets app online (integration): tiles served, see [`TILES`].
    pub(crate) internet_access: bool,
}

/// Hands file to page, which asks reader: API says `sendToChat`, but reader picks destination.
/// Page is on Qt thread (`queued_callback`).
pub(crate) type HandedOver = Arc<dyn Fn(String, String) + Send + Sync>;

/// Shared by all connections.
struct Shared {
    rpc: Arc<RpcClient>,
    instance: Instance,
    /// Handed-over file sink.
    to_chat: HandedOver,
    /// `/webxdc-api/<token>`: chat; app files at root.
    api: String,
    /// Required `Host:`.
    authority: String,
    /// Cleared on close; late connections stop.
    running: Arc<AtomicBool>,
    /// Woken on chat news and host stop.
    changed: Arc<Notify>,
}

/// Served app: URL for `WebView`, serving task. Drop (closing page) stops host.
pub(crate) struct Host {
    /// App URL, for `WebxdcApp.url`.
    url: String,
    running: Arc<AtomicBool>,
    changed: Arc<Notify>,
    task: JoinHandle<()>,
}

impl Host {
    /// Where app is served.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// Chat news: waiting update request re-asks core.
    pub(crate) fn poke(&self) {
        self.changed.notify_waiters();
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Flag stops accepted connections; abort closes listener.
        self.running.store(false, Ordering::SeqCst);
        self.task.abort();
        // Waiting update answers now: its page is gone.
        self.changed.notify_waiters();
    }
}

/// Serve `instance` on loopback until [`Host`] dropped.
///
/// # Errors
///
/// Port can't bind.
pub(crate) async fn start(
    rpc: Arc<RpcClient>,
    instance: Instance,
    to_chat: HandedOver,
) -> Result<Host, String> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .map_err(|err| format!("cannot serve the app: {err}"))?;
    let address = listener
        .local_addr()
        .map_err(|err| format!("cannot serve the app: {err}"))?;
    let token = token();
    let authority = format!("127.0.0.1:{}", address.port());
    // Front page at root.
    let url = format!("http://{authority}/index.html");
    let running = Arc::new(AtomicBool::new(true));
    let changed = Arc::new(Notify::new());
    // Last run's leftovers; see `empty_outbox`.
    empty_outbox(instance.message_id);
    let shared = Arc::new(Shared {
        rpc,
        instance,
        to_chat,
        api: format!("/webxdc-api/{token}"),
        authority,
        running: Arc::clone(&running),
        changed: Arc::clone(&changed),
    });
    let task = tokio::spawn(accept(listener, shared));
    Ok(Host {
        url,
        running,
        changed,
        task,
    })
}

/// Concurrent: page loads script, style, pictures at once; a slow blob would hold up the rest.
async fn accept(listener: TcpListener, shared: Arc<Shared>) {
    while shared.running.load(Ordering::SeqCst) {
        // Listener can't accept: socket gone.
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

/// One request, answer, close. No keep-alive.
async fn answer(mut stream: TcpStream, shared: Arc<Shared>) -> std::io::Result<()> {
    let mut response = match read_head(&mut stream).await? {
        None => Response::empty("400 Bad Request"),
        // Handover streams to disk; held whole needs room twice.
        Some(head) if handover_asked(&shared, &head) => {
            take_handover(&shared, &mut stream, head).await?
        }
        Some(head) => {
            let target = head.target.clone();
            let host = head.host.clone();
            let method = head.method.clone();
            match read_body(&mut stream, head).await? {
                None => Response::empty("413 Payload Too Large"),
                Some(body) => {
                    route(
                        &shared,
                        &Request {
                            method,
                            target,
                            host,
                            body,
                        },
                    )
                    .await
                }
            }
        }
    };
    let after = response.after.take();
    let written = write_response(&mut stream, &response).await;
    // After answer: picker over app hangs it if request open. Skipped if write failed (page gone).
    if let (Ok(()), Some(after)) = (&written, after) {
        after();
    }
    written
}

/// Request fields this host reads.
struct Request {
    method: String,
    target: String,
    host: String,
    body: Vec<u8>,
}

/// Response.
struct Response {
    status: &'static str,
    content_type: String,
    body: Vec<u8>,
    /// Runs after answer is on wire. See `to_chat`.
    after: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl Response {
    fn new(content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status: "200 OK",
            content_type: content_type.to_string(),
            body,
            after: None,
        }
    }

    /// Refusal with reason: blank page looks like hung app.
    fn empty(status: &'static str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: status.as_bytes().to_vec(),
            after: None,
        }
    }

    /// App unreadable: shown to reader.
    fn failed(status: &'static str, reason: &str) -> Self {
        let body = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width\">\
             <title>{status}</title></head>\
             <body style=\"font-family:sans-serif;padding:1.5em;color:#808080\">\
             <p>This app could not be opened.</p><p>{}</p></body></html>",
            escape(reason)
        );
        Self {
            status,
            content_type: "text/html; charset=utf-8".to_string(),
            body: body.into_bytes(),
            after: None,
        }
    }
}

/// Core's words as page text.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Request head: all before body. Shared with `call_host.rs`.
pub(crate) struct Head {
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) host: String,
    /// `Content-Length`, 0 if absent.
    pub(crate) length: usize,
    /// Body bytes already read with head.
    pub(crate) started: Vec<u8>,
}

/// Read head; `None` if unparseable. Body left on socket for caller.
pub(crate) async fn read_head(stream: &mut TcpStream) -> std::io::Result<Option<Head>> {
    let mut head = Vec::new();
    let mut chunk = [0_u8; 2048];
    let body_at = loop {
        if let Some(at) = find(&head, b"\r\n\r\n") {
            break at.saturating_add(4);
        }
        if head.len() > MAX_HEAD {
            return Ok(None);
        }
        let read = stream.read(&mut chunk).await?;
        let Some(arrived) = chunk.get(..read).filter(|arrived| !arrived.is_empty()) else {
            return Ok(None);
        };
        head.extend_from_slice(arrived);
    };

    let started = head.split_off(body_at);
    let Ok(text) = std::str::from_utf8(&head) else {
        return Ok(None);
    };
    let mut lines = text.lines();
    let Some(start) = lines.next() else {
        return Ok(None);
    };
    let mut parts = start.split(' ');
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(None);
    };

    let mut host = String::new();
    let mut length = 0_usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            host = value.to_string();
        } else if name.eq_ignore_ascii_case("content-length") {
            length = value.parse().unwrap_or(0);
        }
    }

    Ok(Some(Head {
        method: method.to_string(),
        target: target.to_string(),
        host,
        length,
        started,
    }))
}

/// Rest of kept body; `None` past cap (still drained: closing on a writer resets it).
pub(crate) async fn read_body(
    stream: &mut TcpStream,
    head: Head,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut chunk = [0_u8; 2048];
    if head.length > MAX_BODY {
        let mut dropped = head.started.len();
        while dropped < head.length.min(MAX_DRAIN) {
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            dropped = dropped.saturating_add(read);
        }
        return Ok(None);
    }
    let mut body = head.started;
    while body.len() < head.length {
        let read = stream.read(&mut chunk).await?;
        let Some(arrived) = chunk.get(..read).filter(|arrived| !arrived.is_empty()) else {
            break;
        };
        body.extend_from_slice(arrived);
    }
    body.truncate(head.length);
    Ok(Some(body))
}

/// Where needle starts in haystack.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Pick answer: unhandled paths are archive files.
async fn route(shared: &Shared, request: &Request) -> Response {
    // Browser reaches 127.0.0.1 too: given address only.
    if !shared.running.load(Ordering::SeqCst) || request.host != shared.authority {
        return Response::empty("404 Not Found");
    }
    let (path, query) = match request.target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (request.target.as_str(), ""),
    };
    // Chat behind token, checked first; missing token refused, not searched.
    if path.starts_with("/webxdc-api") {
        let Some(rest) = path.strip_prefix(&shared.api) else {
            return Response::empty("404 Not Found");
        };
        // `/to-chat` answered before body read (see `handover_asked`).
        return match (request.method.as_str(), rest) {
            ("GET", "/updates") => updates(shared, query).await,
            ("POST", "/send") => send(shared, &request.body).await,
            ("GET", tile) if shared.instance.internet_access => match tile_url(tile) {
                Some(url) => fetch_tile(shared, &url).await,
                None => Response::empty("404 Not Found"),
            },
            _ => Response::empty("404 Not Found"),
        };
    }
    // Bridge answered before archive: app's own `webxdc.js` is shadowed.
    match (request.method.as_str(), path) {
        ("GET", "" | "/" | "/index.html") => index(shared).await,
        ("GET", "/webxdc.js") => Response::new(
            "text/javascript; charset=utf-8",
            bridge(shared).into_bytes(),
        ),
        ("GET", inner) => blob(shared, inner).await,
        _ => Response::empty("404 Not Found"),
    }
}

/// App's front page, API put first.
async fn index(shared: &Shared) -> Response {
    match file(shared, "index.html").await {
        Ok(bytes) => {
            let html = String::from_utf8_lossy(&bytes);
            Response::new("text/html; charset=utf-8", inject(&html).into_bytes())
        }
        // No index.html = no app: core's reason is the useful one.
        Err(reason) => Response::failed("502 Bad Gateway", &reason),
    }
}

/// One archive file, named as asked.
async fn blob(shared: &Shared, path: &str) -> Response {
    let Some(name) = archive_path(path) else {
        return Response::empty("404 Not Found");
    };
    match file(shared, &name).await {
        Ok(bytes)
            if shared.instance.internet_access
                && content_type(&name).starts_with("text/javascript") =>
        {
            let script =
                String::from_utf8_lossy(&bytes).replace(TILES, &format!("{}/tile/", shared.api));
            Response::new(content_type(&name), script.into_bytes())
        }
        Ok(bytes) => Response::new(content_type(&name), bytes),
        Err(_) => Response::empty("404 Not Found"),
    }
}

/// Tile server address for `/tile/{z}/{x}/{y}.png`; digits only, nothing else reaches core.
fn tile_url(path: &str) -> Option<String> {
    let tile = path.strip_prefix("/tile/")?;
    let (numbers, extension) = tile.rsplit_once('.')?;
    let digits = numbers.split('/').collect::<Vec<_>>();
    let well_formed = extension == "png"
        && digits.len() == 3
        && digits
            .iter()
            .all(|part| (1..=7).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_digit()));
    well_formed.then(|| format!("{TILES}{tile}"))
}

/// One tile via core. Not a PNG: refused, page shows gap.
async fn fetch_tile(shared: &Shared, url: &str) -> Response {
    let answer: Result<serde_json::Value, _> = shared
        .rpc
        .call("get_http_response", (shared.instance.account_id, url))
        .await;
    let Ok(answer) = answer else {
        return Response::empty("502 Bad Gateway");
    };
    // Type named here, not taken: server's header text never reaches page's.
    let picture = crate::json::str_at(&answer, "mimetype").starts_with("image/png");
    match decode_base64(crate::json::str_at(&answer, "blob")) {
        Some(bytes) if picture => Response::new(content_type("tile.png"), bytes),
        _ => Response::empty("502 Bad Gateway"),
    }
}

/// Chat updates since `serial`, core JSON unread. `wait=1`: long poll until poke or
/// [`UPDATE_WAIT`]; else core's first answer (bridge catching up, app waits to draw). 404 closed.
async fn updates(shared: &Shared, query: &str) -> Response {
    let serial = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("serial="))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let wait = query.split('&').any(|pair| pair == "wait=1");
    let now = tokio::time::Instant::now();
    let deadline = now.checked_add(UPDATE_WAIT).unwrap_or(now);
    // Listen before look: update landing between still wakes.
    let changed = shared.changed.notified();
    let mut changed = std::pin::pin!(changed);
    changed.as_mut().enable();
    let mut answer = status_updates(shared, serial).await;
    if wait && answer.as_deref().is_some_and(nothing_new) {
        // Ask core either way: wake may not be ours (overflow wakes all apps).
        tokio::time::timeout_at(deadline, changed).await.ok();
        if !shared.running.load(Ordering::SeqCst) {
            return Response::empty("404 Not Found");
        }
        answer = status_updates(shared, serial).await;
    }
    match answer {
        Some(json) => Response::new("application/json; charset=utf-8", json.into_bytes()),
        None => Response::empty("502 Bad Gateway"),
    }
}

/// Core's answer since `serial`; `None` if none.
async fn status_updates(shared: &Shared, serial: u32) -> Option<String> {
    let answer: Result<String, _> = shared
        .rpc
        .call(
            "get_webxdc_status_updates",
            (
                shared.instance.account_id,
                shared.instance.message_id,
                serial,
            ),
        )
        .await;
    answer.ok()
}

/// No brace in answer (`[]` or junk): wait, don't answer, else bridge re-asks in a loop.
fn nothing_new(updates: &str) -> bool {
    !updates.contains('{')
}

/// One update from app, to chat.
async fn send(shared: &Shared, body: &[u8]) -> Response {
    let Ok(update) = std::str::from_utf8(body) else {
        return Response::empty("400 Bad Request");
    };
    // Parsed here: bad update is app's mistake, reported to page.
    if serde_json::from_str::<serde_json::Value>(update).is_err() {
        return Response::empty("400 Bad Request");
    }
    let answer: Result<serde_json::Value, _> = shared
        .rpc
        .call(
            "send_webxdc_status_update",
            (
                shared.instance.account_id,
                shared.instance.message_id,
                update,
                Option::<String>::None,
            ),
        )
        .await;
    match answer {
        Ok(_) => Response::empty("204 No Content"),
        Err(_) => Response::empty("502 Bad Gateway"),
    }
}

/// Handover request? Decided from head: body never read whole.
fn handover_asked(shared: &Shared, head: &Head) -> bool {
    if !shared.running.load(Ordering::SeqCst) || head.host != shared.authority {
        return false;
    }
    let path = head.target.split('?').next().unwrap_or_default();
    head.method == "POST" && path == format!("{}/to-chat", shared.api)
}

/// File and/or text leaving app. File is raw body, streamed to cache; name and text in query.
/// No size limit (off heap; copy removed once saved, outbox emptied at start); full disk:
/// `500`. Path handed on, as pickers do. Name: last component (`../../../etc/passwd` ->
/// `passwd`).
async fn take_handover(
    shared: &Shared,
    stream: &mut TcpStream,
    head: Head,
) -> std::io::Result<Response> {
    let query = head.target.split_once('?').map_or("", |(_, rest)| rest);
    let name = field(query, "name");
    let message = field(query, "text");

    // Text, no file: nothing to write.
    if name.is_empty() {
        drain(stream, head.started.len(), head.length).await?;
        if message.is_empty() {
            return Ok(Response::empty("400 Bad Request"));
        }
        return Ok(handed_over(shared, String::new(), message));
    }

    let Ok(path) = outgoing_path(shared, &name) else {
        drain(stream, head.started.len(), head.length).await?;
        return Ok(Response::empty("400 Bad Request"));
    };
    match write_streamed(stream, &path, &head).await? {
        Written::Done => Ok(handed_over(
            shared,
            path.to_string_lossy().into_owned(),
            message,
        )),
        Written::Failed => {
            drop(tokio::fs::remove_file(&path).await);
            Ok(Response::empty("500 Internal Server Error"))
        }
    }
}

/// How streamed body ended.
enum Written {
    Done,
    Failed,
}

/// Copy body to disk as it arrives.
async fn write_streamed(
    stream: &mut TcpStream,
    path: &std::path::Path,
    head: &Head,
) -> std::io::Result<Written> {
    // tokio file: RPC runtime must not block on slow disk.
    let Ok(mut file) = tokio::fs::File::create(path).await else {
        drain(stream, head.started.len(), head.length).await?;
        return Ok(Written::Failed);
    };

    // Bytes off socket: drain must not re-ask.
    let mut taken = head.started.len();
    let mut chunk = vec![0_u8; 64 * 1024];
    let mut rest: &[u8] = &head.started;
    loop {
        // Failed write usually full disk.
        if !rest.is_empty() && file.write_all(rest).await.is_err() {
            drain(stream, taken, head.length).await?;
            return Ok(Written::Failed);
        }
        if taken >= head.length {
            break;
        }
        let read = stream.read(&mut chunk).await?;
        let Some(arrived) = chunk.get(..read).filter(|arrived| !arrived.is_empty()) else {
            break;
        };
        rest = arrived;
        taken = taken.saturating_add(read);
    }
    if file.flush().await.is_err() {
        return Ok(Written::Failed);
    }
    Ok(Written::Done)
}

/// Read rest of body and drop (closing on a writer resets it). `taken`: bytes already read.
async fn drain(stream: &mut TcpStream, taken: usize, length: usize) -> std::io::Result<()> {
    let mut dropped = taken;
    let mut chunk = [0_u8; 8192];
    while dropped < length.min(MAX_DRAIN) {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        dropped = dropped.saturating_add(read);
    }
    Ok(())
}

/// Handover answer; page told once on wire.
fn handed_over(shared: &Shared, path: String, message: String) -> Response {
    let to_chat = Arc::clone(&shared.to_chat);
    let mut response = Response::empty("204 No Content");
    response.after = Some(Box::new(move || to_chat(path, message)));
    response
}

/// One query field, percent-decoded.
fn field(query: &str, name: &str) -> String {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| percent_decode(value))
        .unwrap_or_default()
}

/// Handover file path in cache, app's name; caller writes.
fn outgoing_path(shared: &Shared, name: &str) -> Result<std::path::PathBuf, String> {
    let name = std::path::Path::new(name)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty() && name != "." && name != "..")
        .ok_or_else(|| format!("{name} is not a file name"))?;
    Ok(outbox_dir(shared.instance.message_id)?.join(name))
}

/// Waiting dir per instance: only dir anything here deletes.
pub(crate) const OUTBOX_DIR: &str = "webxdc/outbox";

/// Instance outbox, created if absent.
fn outbox_dir(message_id: u32) -> Result<std::path::PathBuf, String> {
    // `cache_file` creates parent: asking makes outbox.
    let inside = crate::qr::cache_file(&format!("{OUTBOX_DIR}/{message_id}/file"))?;
    inside
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "the outbox has no directory of its own".to_string())
}

/// Delete cached handover once saved. Path from page: must be directly in own outbox.
pub(crate) fn discard_outgoing(message_id: u32, path: &str) -> bool {
    let Ok(dir) = outbox_dir(message_id) else {
        return false;
    };
    let path = std::path::Path::new(path);
    if !in_outbox(&dir, path) {
        return false;
    }
    std::fs::remove_file(path).is_ok()
}

/// `path` directly in outbox? Guards `remove_file`.
fn in_outbox(dir: &std::path::Path, path: &std::path::Path) -> bool {
    !dir.as_os_str().is_empty() && path.parent() == Some(dir)
}

/// Empty outbox of closed or failed handovers. Called at start: none in flight.
pub(crate) fn empty_outbox(message_id: u32) {
    if let Ok(dir) = outbox_dir(message_id) {
        drop(std::fs::remove_dir_all(dir));
    }
}

/// One archive file via core; error is core's words.
async fn file(shared: &Shared, name: &str) -> Result<Vec<u8>, String> {
    let answer: Result<String, _> = shared
        .rpc
        .call(
            "get_webxdc_blob",
            (shared.instance.account_id, shared.instance.message_id, name),
        )
        .await;
    let encoded = answer.map_err(|err| err.to_string())?;
    decode_base64(&encoded).ok_or_else(|| format!("{name} came back as something unreadable"))
}

/// Bridge with instance values.
fn bridge(shared: &Shared) -> String {
    fill(BRIDGE, shared)
}

/// Placeholders as JSON literals: quotes in names stay strings.
fn fill(script: &str, shared: &Shared) -> String {
    let text = |value: &str| serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string());
    script
        .replace("__API__", &text(&shared.api))
        .replace("__SELF_ADDR__", &text(&shared.instance.self_addr))
        .replace("__SELF_NAME__", &text(&shared.instance.self_name))
        .replace(
            "__INTERVAL__",
            &shared.instance.send_update_interval.to_string(),
        )
        .replace(
            "__MAX_SIZE__",
            &shared.instance.send_update_max_size.to_string(),
        )
        .replace("__RETRY__", &RETRY_INTERVAL.to_string())
}

/// index.html with API script first in `<head>`, or at front.
fn inject(html: &str) -> String {
    const TAG: &str = "<script src=\"webxdc.js\"></script>";
    // ASCII only: offsets must match `html`; full lowercase changes length.
    let lower = html.to_ascii_lowercase();
    let at = lower
        .find("<head")
        .and_then(|start| {
            let end = lower.get(start..)?.find('>')?;
            Some(start.saturating_add(end).saturating_add(1))
        })
        .unwrap_or(0);
    let (before, after) = match (html.get(..at), html.get(at..)) {
        (Some(before), Some(after)) => (before, after),
        _ => ("", html),
    };
    let mut out = String::with_capacity(html.len().saturating_add(TAG.len()));
    out.push_str(before);
    out.push_str(TAG);
    out.push_str(after);
    out
}

/// Archive entry from request path: decoded, no leading slash, no escape.
fn archive_path(path: &str) -> Option<String> {
    let path = percent_decode(path.strip_prefix('/').unwrap_or(path));
    if path.is_empty() || path.starts_with('/') {
        return None;
    }
    // Zip names anything; `../` refused here.
    if path
        .split('/')
        .any(|segment| segment == ".." || segment == ".")
    {
        return None;
    }
    Some(path)
}

/// `%20` etc. Undecodable UTF-8 kept as written.
fn percent_decode(text: &str) -> String {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        let escape = match tail {
            [high, low, after @ ..] if byte == b'%' => std::str::from_utf8(&[*high, *low])
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .map(|value| (value, after)),
            _ => None,
        };
        if let Some((value, after)) = escape {
            out.push(value);
            rest = after;
        } else {
            out.push(byte);
            rest = tail;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// Content type by extension; rest downloads as bytes.
pub(crate) fn content_type(name: &str) -> &'static str {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "text/xml; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp3" => "audio/mpeg",
        "oga" | "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// Standard base64, padding optional (`STANDARD_NO_PAD`). Whitespace skipped.
pub(crate) fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity((text.len() / 4).saturating_mul(3));
    let mut buffer = 0_u32;
    let mut bits = 0_u32;
    for byte in text.bytes() {
        // Ranges count on: lower from 26, digits from 52.
        let value = match byte {
            b'A'..=b'Z' => byte.checked_sub(b'A')?,
            b'a'..=b'z' => byte.checked_sub(b'a' - 26)?,
            b'0'..=b'9' => byte.checked_add(52 - b'0')?,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        // Never past 13: eight come off at eight.
        bits = bits.saturating_add(6);
        if let Some(left) = bits.checked_sub(8) {
            bits = left;
            // Completed byte; drop bits of next.
            let shifted = buffer >> bits;
            out.push(u8::try_from(shifted & 0xff).ok()?);
        }
    }
    Some(out)
}

/// Digits a token is written in.
const HEX: &[u8; 16] = b"0123456789abcdef";

/// Unguessable part of every path, from `/dev/urandom`. Fallback fine: device-only reach.
pub(crate) fn token() -> String {
    let mut bytes = [0_u8; 16];
    let read = std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_ok();
    if !read {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.subsec_nanos());
        let mixed = u64::from(nanos) ^ (u64::from(std::process::id()) << 32);
        bytes[..8].copy_from_slice(&mixed.to_le_bytes());
    }
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        // Hex by hand: no `format!` per byte.
        for nibble in [byte >> 4, byte & 0x0f] {
            if let Some(&digit) = HEX.get(usize::from(nibble)) {
                out.push(char::from(digit));
            }
        }
    }
    out
}

/// Writes answer with policy: own origin, what it may do.
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
        archive_path, content_type, decode_base64, in_outbox, inject, nothing_new, percent_decode,
        token, Response,
    };

    #[test]
    fn an_answer_with_no_update_in_it_is_waited_on() {
        assert!(nothing_new("[]"));
        assert!(nothing_new(" [ ]\n"));
        assert!(!nothing_new("[{\"payload\":1,\"serial\":1}]"));
        // Nothing usable: answered at once, bridge re-asks at once.
        assert!(nothing_new(""));
        assert!(nothing_new("null"));
    }

    #[test]
    fn only_a_file_in_the_outbox_itself_is_ever_deleted() {
        let dir = std::path::Path::new("/cache/piirit/webxdc/outbox/5");
        let inside = |name: &str| in_outbox(dir, std::path::Path::new(name));

        assert!(inside("/cache/piirit/webxdc/outbox/5/notes.txt"));
        // Name kept whole, quirks and all.
        assert!(inside("/cache/piirit/webxdc/outbox/5/two words.mp4"));

        // Other app's outbox.
        assert!(!inside("/cache/piirit/webxdc/outbox/6/notes.txt"));
        // Outbox and cache around it.
        assert!(!inside("/cache/piirit/webxdc/outbox/5"));
        assert!(!inside("/cache/piirit/webxdc/outbox"));
        // Below it, not in it.
        assert!(!inside("/cache/piirit/webxdc/outbox/5/deeper/notes.txt"));
        // Reader's own files.
        assert!(!inside("/elsewhere/Downloads/notes.txt"));
        assert!(!inside(
            "/cache/piirit/webxdc/outbox/5/../../../../etc/passwd"
        ));
        assert!(!inside("notes.txt"));
        assert!(!inside(""));
    }

    #[test]
    fn base64_reads_with_or_without_padding_and_refuses_anything_else() {
        assert_eq!(decode_base64("aGk").as_deref(), Some(&b"hi"[..]));
        assert_eq!(decode_base64("aGk=").as_deref(), Some(&b"hi"[..]));
        assert_eq!(decode_base64("aGVsbG8=").as_deref(), Some(&b"hello"[..]));
        assert_eq!(decode_base64("").as_deref(), Some(&b""[..]));
        // Core wraps nothing; wrapped blob is still a blob.
        assert_eq!(decode_base64("aGVs\r\nbG8").as_deref(), Some(&b"hello"[..]));
        assert_eq!(decode_base64("not base64!"), None);
    }

    #[test]
    fn base64_round_trips_every_byte() {
        // Hand-encoded: decoder checked against alphabet.
        let bytes: Vec<u8> = (0..=255).collect();
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut encoded = String::new();
        for chunk in bytes.chunks(3) {
            let mut block = [0_u8; 3];
            block[..chunk.len()].copy_from_slice(chunk);
            let packed =
                (u32::from(block[0]) << 16) | (u32::from(block[1]) << 8) | u32::from(block[2]);
            for index in 0..=chunk.len() {
                let shift = 18 - 6 * index;
                encoded.push(alphabet[((packed >> shift) & 0x3f) as usize] as char);
            }
        }
        assert_eq!(decode_base64(&encoded).as_deref(), Some(&bytes[..]));
    }

    #[test]
    fn the_api_goes_inside_the_head_and_before_every_script() {
        let injected = inject("<html><head><title>x</title></head><body></body></html>");
        assert_eq!(
            injected,
            "<html><head><script src=\"webxdc.js\"></script><title>x</title></head>\
             <body></body></html>"
        );
        // Head with attributes is still a head.
        assert!(inject("<HEAD lang=\"en\">a")
            .starts_with("<HEAD lang=\"en\"><script src=\"webxdc.js\"></script>"));
        // Document without one gets API first.
        assert!(inject("<p>hi</p>").starts_with("<script src=\"webxdc.js\"></script>"));
    }

    #[test]
    fn text_that_lowercases_longer_does_not_move_the_cut() {
        // "İ" lowercases to three bytes; cut panicked in lowercased copy.
        assert_eq!(
            inject("İ<head>"),
            "İ<head><script src=\"webxdc.js\"></script>"
        );
        // Same shift, inside a character.
        assert_eq!(
            inject("İ<head>é"),
            "İ<head><script src=\"webxdc.js\"></script>é"
        );
    }

    #[test]
    fn a_path_cannot_leave_the_archive() {
        assert_eq!(archive_path("/index.html").as_deref(), Some("index.html"));
        assert_eq!(
            archive_path("/assets/app%20one.js").as_deref(),
            Some("assets/app one.js")
        );
        assert_eq!(archive_path("/../../etc/passwd"), None);
        assert_eq!(archive_path("/assets/../../secret"), None);
        assert_eq!(archive_path("/./index.html"), None);
        assert_eq!(archive_path("/"), None);
    }

    #[test]
    fn percent_decoding_leaves_what_is_not_an_escape_alone() {
        assert_eq!(percent_decode("a%2Fb"), "a/b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("a%zzb"), "a%zzb");
    }

    #[test]
    fn a_page_is_named_by_its_extension_and_anything_else_is_bytes() {
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(content_type("app.JS"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("icon.png"), "image/png");
        assert_eq!(content_type("data"), "application/octet-stream");
        assert_eq!(content_type("archive.tar.gz"), "application/octet-stream");
    }

    #[test]
    fn a_refusal_says_what_happened_rather_than_drawing_blank() {
        // Every refusal has reason: empty body = blank page.
        let refused = Response::empty("404 Not Found");
        assert_eq!(refused.body, b"404 Not Found");

        // App's page worth a sentence; core words go in as text.
        let failed = Response::failed("502 Bad Gateway", "no <b>file</b> & no message");
        let page = String::from_utf8_lossy(&failed.body);
        assert!(
            page.contains("This app could not be opened."),
            "the failure page does not say what went wrong: {page}"
        );
        assert!(
            page.contains("no &lt;b&gt;file&lt;/b&gt; &amp; no message"),
            "the core's words were not escaped into the page: {page}"
        );
        assert_eq!(failed.content_type, "text/html; charset=utf-8");
    }

    #[test]
    fn two_tokens_are_not_the_same_token() {
        let (one, two) = (token(), token());
        assert_eq!(one.len(), 32);
        assert_ne!(one, two);
    }
}
