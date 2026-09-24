//! The loopback host one webxdc app is served from while it is open.
//!
//! A webxdc app is a zip file, and the `WebView` needs its files as a
//! website: an index.html that can pull in its own scripts, styles and
//! pictures. Nothing here opens the zip. Every file comes from the core,
//! which reads the archive itself (`get_webxdc_blob`) -- so the app is
//! served rather than unpacked, and no archive format is parsed in this
//! repository.
//!
//! Serving it over HTTP is also what gives the app a way back: the API in
//! `webxdc.js` sends an update with a POST and collects the others by
//! polling, so the bridge needs nothing of the `WebView` beyond the URL it
//! is pointed at. The alternative -- a Gecko frame script -- would put
//! the bridge on chrome-privileged ground this app cannot test.
//!
//! The app is served from the root of that address, as every other
//! client serves one. It has to be: an app built with a bundler's
//! default settings asks for `/assets/index-1a2b.js`, and a host that
//! keeps its files under a prefix answers nothing to that -- which is a
//! blank screen for half the apps in the store and a working one for the
//! half that happens to use relative paths.
//!
//! What keeps that from being a hole in the phone:
//!
//! - the listener is bound to 127.0.0.1 on a port the kernel picks, so
//!   nothing off the device can reach it at all;
//! - a request whose `Host:` is not the address the app was given is
//!   refused, so a name that resolves to 127.0.0.1 is not a way in;
//! - the chat is behind an unguessable token: the two API paths -- what
//!   everyone else in the chat has said, and saying something to them --
//!   are under `/webxdc-api/<token>/`, which the app is told and nothing
//!   else can guess. A page that found the port could ask for the app's
//!   own files, which its reader was sent anyway; it cannot read the
//!   chat or write to it.
//! - it exists only while the app is open, and answers nothing once the
//!   page is closed;
//! - the app itself is confined by a content-security-policy that permits
//!   this origin and nothing else, so a webxdc cannot reach the network
//!   even though it is on one.

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use deltachat_jsonrpc::RpcClient;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// The API the app sees, as JavaScript. Compiled in rather than installed
/// beside the QML: it is served, not loaded by the engine, and a file the
/// package could lose is a bridge that fails on a phone and nowhere else.
const BRIDGE: &str = include_str!("webxdc.js");

/// What an app may do from inside the `WebView`. `'self'` is this host,
/// which is where its own files and the two API paths are; nothing else
/// is reachable, so an app cannot phone home whatever it asks for.
/// `unsafe-inline` and `unsafe-eval` are for the app's own code -- webxdc
/// apps are single files full of inline script, and the isolation here
/// comes from the origin having nowhere to go rather than from what the
/// app is allowed to run.
const POLICY: &str = "default-src 'self'; \
     script-src 'self' 'unsafe-inline' 'unsafe-eval' blob:; \
     style-src 'self' 'unsafe-inline' blob:; \
     img-src 'self' data: blob:; \
     media-src 'self' data: blob:; \
     font-src 'self' data: blob:; \
     connect-src 'self' data: blob:; \
     form-action 'none'; \
     frame-ancestors 'none'";

/// Milliseconds the bridge waits between asking for updates.
const POLL_INTERVAL: u32 = 300;

/// The most a request head may be. A webxdc's own requests are short;
/// anything longer is not one of them.
const MAX_HEAD: usize = 16 * 1024;

/// The most a `POST`ed update may be. The core's own soft limit is 100 KiB
/// and it rejects what is over its own; this is the point at which the
/// body is not read at all.
const MAX_BODY: usize = 512 * 1024;

/// The most of a refused body to read before answering.
///
/// A request whose body is not going to be read still has to be listened
/// to: answering and closing while the other end is still writing resets
/// the connection, and a reset is not an answer -- the app sees a host it
/// could not reach and has no idea why. So the body is read and dropped
/// first, up to this, which is generous enough for any refusal that is
/// really an app's mistake and small enough not to sit here forever.
const MAX_DRAIN: usize = 256 * 1024 * 1024;

/// Which app is being served, and what it is told about itself.
pub(crate) struct Instance {
    /// The account the message belongs to.
    pub(crate) account_id: u32,
    /// The message the webxdc arrived as, which is also the instance the
    /// core keeps status updates for.
    pub(crate) message_id: u32,
    /// `window.webxdc.selfAddr`: what other members of the chat see this
    /// app's updates as coming from. The core's own answer, not the
    /// account address.
    pub(crate) self_addr: String,
    /// `window.webxdc.selfName`: the profile's display name.
    pub(crate) self_name: String,
    /// `window.webxdc.sendUpdateInterval`, in milliseconds.
    pub(crate) send_update_interval: u64,
    /// `window.webxdc.sendUpdateMaxSize`, in bytes.
    pub(crate) send_update_max_size: u64,
}

/// What the host does with a file an app hands over: give it to the page,
/// which asks the reader what they want done with it.
///
/// A callback rather than a core call, because this one is not the
/// host's to make. The API's own word for it is `sendToChat`, but the
/// destination is the reader's and a chat is not the one they mean by a
/// download -- so what the app hands over reaches the page, and the page
/// is on the Qt thread, where a `queued_callback` puts it.
pub(crate) type HandedOver = Arc<dyn Fn(String, String) + Send + Sync>;

/// Everything a connection needs, shared by every one of them.
struct Shared {
    rpc: Arc<RpcClient>,
    instance: Instance,
    /// Where a file the app hands over goes; see [`HandedOver`].
    to_chat: HandedOver,
    /// `/webxdc-api/<token>`: where the chat is, and the one part of
    /// this host that cannot be guessed. The app's own files are at the
    /// root beside it.
    api: String,
    /// The `Host:` a request must carry: the address the app was given.
    authority: String,
    /// Cleared when the app is closed. A connection accepted just before
    /// that stops rather than reaching the core.
    running: Arc<AtomicBool>,
}

/// A served app: the URL to point a `WebView` at, and the task serving it.
///
/// Dropping it stops the host, which is what closing the page does.
pub(crate) struct Host {
    /// Where the app is. What `WebxdcApp.url` hands to QML.
    url: String,
    running: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

impl Host {
    /// Where the app is being served.
    pub(crate) fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Both: the flag stops a connection already accepted, and the
        // abort closes the listener so no more arrive.
        self.running.store(false, Ordering::SeqCst);
        self.task.abort();
    }
}

/// Serve `instance` on the loopback interface until the [`Host`] is
/// dropped.
///
/// # Errors
///
/// Fails when the loopback port cannot be bound.
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
    // The app's front page, at the root: what it asks for next is its
    // own business, and half of what apps ask for is absolute.
    let url = format!("http://{authority}/index.html");
    let running = Arc::new(AtomicBool::new(true));
    // Whatever the last run left behind; see `empty_outbox`.
    empty_outbox(instance.message_id);
    let shared = Arc::new(Shared {
        rpc,
        instance,
        to_chat,
        api: format!("/webxdc-api/{token}"),
        authority,
        running: Arc::clone(&running),
    });
    let task = tokio::spawn(accept(listener, shared));
    Ok(Host { url, running, task })
}

/// One connection at a time is not enough: a page loads its script, its
/// style and its pictures at once, and a blob the core is still reading
/// would hold up the rest.
async fn accept(listener: TcpListener, shared: Arc<Shared>) {
    while shared.running.load(Ordering::SeqCst) {
        // A listener that cannot accept is not going to start: this is
        // the socket being gone, not one bad connection.
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            let _ = answer(stream, shared).await;
        });
    }
}

/// Read one request, answer it, and close. No keep-alive: every answer
/// says so, and a page load is a handful of short connections.
async fn answer(mut stream: TcpStream, shared: Arc<Shared>) -> std::io::Result<()> {
    let mut response = match read_head(&mut stream).await? {
        None => Response::empty("400 Bad Request"),
        // A file an app is handing over is written out as it arrives:
        // the point of the route is files, and a file this host held
        // whole would be a file the phone has to find room for twice.
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
    // After the answer, never before it: what this sets off opens a page
    // over the app, and an app whose request is still outstanding when
    // that happens is an app that never hears back.
    //
    // And only if the answer arrived. A write that failed is a page that
    // is no longer there to have asked, and a picker pushed for it would
    // land over whatever the reader is looking at instead.
    if let (Ok(()), Some(after)) = (&written, after) {
        after();
    }
    written
}

/// One request, as much of it as anything here reads.
struct Request {
    method: String,
    target: String,
    host: String,
    body: Vec<u8>,
}

/// What goes back.
struct Response {
    status: &'static str,
    content_type: String,
    body: Vec<u8>,
    /// What to do once this answer is on the wire, for the one route
    /// whose answer has to arrive before what it sets off does. See
    /// `to_chat`.
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

    /// A refusal, carrying its own reason. An answer with nothing in it
    /// draws as a blank page, and a blank page in a `WebView` cannot be
    /// told apart from an app that came up and did nothing.
    fn empty(status: &'static str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: status.as_bytes().to_vec(),
            after: None,
        }
    }

    /// The app itself could not be read. This fills the view in its
    /// place, so a reader is told what happened rather than shown grey.
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

/// The core's own words, put in a page as text rather than as markup.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The head of one request: everything before its body. Shared with the
/// call host (`call_host.rs`), which reads requests the same way.
pub(crate) struct Head {
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) host: String,
    /// What `Content-Length` said, or 0 when it said nothing.
    pub(crate) length: usize,
    /// The body's first bytes, which arrived with the head.
    pub(crate) started: Vec<u8>,
}

/// Read the head, or `None` for something that does not parse as one.
///
/// The body is left on the socket. What it is for decides how it is
/// read: a file an app is handing over is written out as it arrives and
/// never held, and everything else on this host is short enough to keep.
pub(crate) async fn read_head(stream: &mut TcpStream) -> std::io::Result<Option<Head>> {
    let mut head = Vec::new();
    let mut chunk = [0_u8; 2048];
    let body_at = loop {
        if let Some(at) = find(&head, b"\r\n\r\n") {
            break at + 4;
        }
        if head.len() > MAX_HEAD {
            return Ok(None);
        }
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(None);
        }
        head.extend_from_slice(&chunk[..read]);
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

/// The rest of a body this host is going to keep, or `None` past the cap.
///
/// Past it the body is still read, and dropped as it comes: answering and
/// closing on a client that is still writing resets the connection, and a
/// reset is not an answer -- the app sees a host it could not reach and
/// has no idea why.
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
            dropped += read;
        }
        return Ok(None);
    }
    let mut body = head.started;
    while body.len() < head.length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(head.length);
    Ok(Some(body))
}

/// Where the needle starts in the haystack.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Pick the answer. Everything this host does not itself answer is a file
/// inside the archive, which the core reads.
async fn route(shared: &Shared, request: &Request) -> Response {
    // A page in the browser can reach 127.0.0.1 too, and a name that
    // resolves there would arrive with its own Host. Only the address the
    // app was handed is served.
    if !shared.running.load(Ordering::SeqCst) || request.host != shared.authority {
        return Response::empty("404 Not Found");
    }
    let (path, query) = match request.target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (request.target.as_str(), ""),
    };
    // The chat, behind the token. Checked before anything else and
    // answered by nothing else: a request under this prefix that does not
    // carry the token is refused rather than looked for in the archive,
    // so guessing is told nothing.
    if path.starts_with("/webxdc-api") {
        let Some(rest) = path.strip_prefix(&shared.api) else {
            return Response::empty("404 Not Found");
        };
        // `/to-chat` is not here: a handover is answered before its body
        // is read, so it never reaches this (see `handover_asked`).
        return match (request.method.as_str(), rest) {
            ("GET", "/updates") => updates(shared, query).await,
            ("POST", "/send") => send(shared, &request.body).await,
            _ => Response::empty("404 Not Found"),
        };
    }
    // The bridge is answered before anything is looked for in the
    // archive, so an app that ships a `webxdc.js` of its own -- some do,
    // to run outside a messenger -- gets this one.
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

/// The app's own front page, with the API put in front of it.
async fn index(shared: &Shared) -> Response {
    match file(shared, "index.html").await {
        Ok(bytes) => {
            let html = String::from_utf8_lossy(&bytes);
            Response::new("text/html; charset=utf-8", inject(&html).into_bytes())
        }
        // The one refusal worth spelling out: without index.html there
        // is no app, and the core's reason is the only thing anybody can
        // act on. A blank page here reads as a broken app.
        Err(reason) => Response::failed("502 Bad Gateway", &reason),
    }
}

/// One file out of the archive, named as the app asked for it.
async fn blob(shared: &Shared, path: &str) -> Response {
    let Some(name) = archive_path(path) else {
        return Response::empty("404 Not Found");
    };
    match file(shared, &name).await {
        Ok(bytes) => Response::new(content_type(&name), bytes),
        Err(_) => Response::empty("404 Not Found"),
    }
}

/// Everything the chat has said about this app since `serial`, as the
/// core hands it over: a JSON array, passed through unread.
async fn updates(shared: &Shared, query: &str) -> Response {
    let serial = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("serial="))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
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
    match answer {
        Ok(json) => Response::new("application/json; charset=utf-8", json.into_bytes()),
        Err(_) => Response::empty("502 Bad Gateway"),
    }
}

/// One update from the app, on its way to everyone else in the chat.
async fn send(shared: &Shared, body: &[u8]) -> Response {
    let Ok(update) = std::str::from_utf8(body) else {
        return Response::empty("400 Bad Request");
    };
    // Read here rather than passed on as text: an update the core cannot
    // parse is the app's mistake, and it should hear about it from the
    // host it is talking to rather than through a core error nothing in
    // the page can see.
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

/// Whether this request is an app handing something over.
///
/// Answered from the head alone, because the body has not been read yet
/// and for this one route it never will be as a whole: what the app
/// sends goes to the disk as it arrives.
fn handover_asked(shared: &Shared, head: &Head) -> bool {
    if !shared.running.load(Ordering::SeqCst) || head.host != shared.authority {
        return false;
    }
    let path = head.target.split('?').next().unwrap_or_default();
    head.method == "POST" && path == format!("{}/to-chat", shared.api)
}

/// A file, a piece of text, or both, on their way out of the app.
///
/// The file *is* the request body -- no base64, no JSON around it -- so
/// it is copied from the socket into the cache a chunk at a time and
/// never held. What it is called and what goes with it are in the query,
/// which the head already carried.
///
/// There is no size limit. The one there was existed to keep the file
/// out of the heap, and streaming keeps it out already; what was left of
/// it was a number standing in for a cleanup that did not exist. The
/// cleanup exists now -- the copy goes as soon as it is saved, and the
/// outbox is emptied when the app starts -- and the disk answers for
/// itself: a write with no room left for it is a `500`, which is the
/// truth and is an answer the app can show.
///
/// The *path* is handed on, not the bytes: what receives it is a page,
/// and a page takes a file by path the way every other picker in this app
/// hands one over. Nothing is opened or saved from here.
///
/// The name is the app's, so it is taken apart and only its last
/// component kept: an app that asks to write `../../../etc/passwd` gets
/// a file called `passwd` in the cache.
async fn take_handover(
    shared: &Shared,
    stream: &mut TcpStream,
    head: Head,
) -> std::io::Result<Response> {
    let query = head.target.split_once('?').map_or("", |(_, rest)| rest);
    let name = field(query, "name");
    let message = field(query, "text");

    // Text with no file: nothing to write, and nothing to read either.
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

/// How a streamed body ended.
enum Written {
    Done,
    Failed,
}

/// Copy the body onto the disk as it arrives, or say why not.
async fn write_streamed(
    stream: &mut TcpStream,
    path: &std::path::Path,
    head: &Head,
) -> std::io::Result<Written> {
    // tokio's file rather than std's: this runs on the runtime the
    // JSON-RPC client shares, and a write to a full or slow disk would
    // otherwise hold one of its threads for the duration.
    let Ok(mut file) = tokio::fs::File::create(path).await else {
        drain(stream, head.started.len(), head.length).await?;
        return Ok(Written::Failed);
    };

    // What has come off the socket, which is what a drain must not ask
    // for again -- not the same as what has reached the file.
    let mut taken = head.started.len();
    let mut chunk = vec![0_u8; 64 * 1024];
    let mut rest: &[u8] = &head.started;
    loop {
        // A write that fails is the disk being full as often as not,
        // which is the only ceiling this route has left.
        if !rest.is_empty() && file.write_all(rest).await.is_err() {
            drain(stream, taken, head.length).await?;
            return Ok(Written::Failed);
        }
        if taken >= head.length {
            break;
        }
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        rest = &chunk[..read];
        taken += read;
    }
    if file.flush().await.is_err() {
        return Ok(Written::Failed);
    }
    Ok(Written::Done)
}

/// Read whatever is left of a body and drop it.
///
/// `taken` is how much of it has already come off the socket, so that a
/// body half-read is not waited on twice over -- which is a wait for
/// bytes the other end has already sent and will not send again.
///
/// Answering and closing on a client that is still writing resets the
/// connection, and a reset is not an answer: the app sees a host it could
/// not reach and has no idea why.
async fn drain(stream: &mut TcpStream, taken: usize, length: usize) -> std::io::Result<()> {
    let mut dropped = taken;
    let mut chunk = [0_u8; 8192];
    while dropped < length.min(MAX_DRAIN) {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        dropped += read;
    }
    Ok(())
}

/// The answer to a handover, with the page told once it is on the wire.
fn handed_over(shared: &Shared, path: String, message: String) -> Response {
    let to_chat = Arc::clone(&shared.to_chat);
    let mut response = Response::empty("204 No Content");
    response.after = Some(Box::new(move || to_chat(path, message)));
    response
}

/// One field out of a query string, percent-decoded.
fn field(query: &str, name: &str) -> String {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| percent_decode(value))
        .unwrap_or_default()
}

/// Where a file an app is handing over is written, under a name of its
/// own choosing but nowhere of its own choosing.
///
/// The directory is made here and the file is opened by the caller, which
/// then writes it as it arrives.
fn outgoing_path(shared: &Shared, name: &str) -> Result<std::path::PathBuf, String> {
    let name = std::path::Path::new(name)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty() && name != "." && name != "..")
        .ok_or_else(|| format!("{name} is not a file name"))?;
    Ok(outbox_dir(shared.instance.message_id)?.join(name))
}

/// Where under the cache a file an app hands over waits for the page to
/// save it: one directory per app instance, and the only directory
/// anything here will delete from.
pub(crate) const OUTBOX_DIR: &str = "webxdc/outbox";

/// This instance's outbox, made if it is not there yet.
fn outbox_dir(message_id: u32) -> Result<std::path::PathBuf, String> {
    // `cache_file` makes the parent of whatever it is asked for, so
    // asking it for a file in the outbox is what makes the outbox.
    let inside = crate::qr::cache_file(&format!("{OUTBOX_DIR}/{message_id}/file"))?;
    inside
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "the outbox has no directory of its own".to_string())
}

/// Delete a file this app handed over, now that it has been saved.
///
/// What the page saves is a copy, so once it is saved the cache holds
/// the same bytes a second time and nothing will ever ask for them. On a
/// phone that second copy is the whole cost of the feature, and it is
/// this call that stops it being permanent.
///
/// The path comes back from the page rather than being remembered here,
/// so it is checked rather than trusted: anything that is not a file
/// directly in this instance's own outbox is left where it is.
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

/// Whether `path` names a file the outbox itself holds.
///
/// Directly in it, so a directory below it does not count and neither
/// does a `..` climbing back out: this is the whole of what stands
/// between a path the page passed on and `remove_file`, so it compares
/// rather than searches.
fn in_outbox(dir: &std::path::Path, path: &std::path::Path) -> bool {
    !dir.as_os_str().is_empty() && path.parent() == Some(dir)
}

/// Empty this instance's outbox.
///
/// What is left in it is a handover nothing ever saved -- the app was
/// closed with one in flight, or the copy failed -- and it would sit
/// there for as long as the phone did. An app starting is the moment to
/// clear it: it is the one point at which none of its own handovers can
/// be in flight.
pub(crate) fn empty_outbox(message_id: u32) {
    if let Ok(dir) = outbox_dir(message_id) {
        drop(std::fs::remove_dir_all(dir));
    }
}

/// One file from the archive, through the core. The error is the core's
/// own words: the message is gone, the archive holds no such name, or the
/// attachment was never downloaded.
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

/// The bridge with this instance's own values in it.
fn bridge(shared: &Shared) -> String {
    fill(BRIDGE, shared)
}

/// The placeholders in `webxdc.js`, as JSON literals: a name with a quote
/// in it is a string either way rather than the end of one.
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
        .replace("__POLL__", &POLL_INTERVAL.to_string())
}

/// The app's index.html with the API loaded before any of its own code.
///
/// Inside `<head>` where there is one, since that is where a document's
/// scripts are expected and where anything the app runs on load comes
/// after it. A document without a head gets it at the front, which is
/// still before every script in the body.
fn inject(html: &str) -> String {
    const TAG: &str = "<script src=\"webxdc.js\"></script>";
    let lower = html.to_lowercase();
    let at = lower
        .find("<head")
        .and_then(|start| lower[start..].find('>').map(|end| start + end + 1))
        .unwrap_or(0);
    let mut out = String::with_capacity(html.len() + TAG.len());
    out.push_str(&html[..at]);
    out.push_str(TAG);
    out.push_str(&html[at..]);
    out
}

/// The name of a file inside the archive, from the path a request asked
/// for: percent-decoded, without its leading slash, and without a way
/// out of the archive.
fn archive_path(path: &str) -> Option<String> {
    let path = percent_decode(path.strip_prefix('/').unwrap_or(path));
    if path.is_empty() || path.starts_with('/') {
        return None;
    }
    // A zip may name an entry anything at all, and the core looks up what
    // it is given: "../" is refused here rather than trusted to mean
    // nothing there.
    if path
        .split('/')
        .any(|segment| segment == ".." || segment == ".")
    {
        return None;
    }
    Some(path)
}

/// `%20` and the rest. A byte that is not valid UTF-8 once decoded
/// leaves the path as it was written, which then simply does not name
/// anything in the archive.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let decoded = if byte == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|pair| u8::from_str_radix(pair, 16).ok());
            match hex {
                Some(value) => {
                    index += 3;
                    Some(value)
                }
                None => None,
            }
        } else {
            None
        };
        if let Some(value) = decoded {
            out.push(value);
        } else {
            out.push(byte);
            index += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// What to call a file, by its extension. Only what a page is built from;
/// anything else is handed over as bytes, which is what a download is.
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

/// Standard base64, with or without its padding: what the core encodes a
/// blob as (`STANDARD_NO_PAD`). Whitespace is skipped; anything else that
/// is not in the alphabet is a blob this cannot read.
pub(crate) fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a') + 26,
            b'0'..=b'9' => u32::from(byte - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' | b'\t' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            // The byte that just completed, dropping the bits that are
            // still only part of the next one.
            let shifted = buffer >> bits;
            out.push(u8::try_from(shifted & 0xff).ok()?);
        }
    }
    Some(out)
}

/// The digits a token is written in.
const HEX: &[u8; 16] = b"0123456789abcdef";

/// The unguessable part of every path.
///
/// From `/dev/urandom`, which is there on every phone this runs on. The
/// fallback is not a secret worth relying on, and does not have to be:
/// what it guards is one app's files on a socket only this device can
/// reach, and the app is open for as long as somebody is looking at it.
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
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Two hex digits, written rather than formatted: `format!` per
        // byte builds sixteen little strings to throw away.
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// Write the answer out. Every one of them carries the policy: the app is
/// on an origin of its own, and this is what it may do from there.
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
        archive_path, content_type, decode_base64, in_outbox, inject, percent_decode, token,
        Response,
    };

    #[test]
    fn only_a_file_in_the_outbox_itself_is_ever_deleted() {
        let dir = std::path::Path::new("/cache/piirit/webxdc/outbox/5");
        let inside = |name: &str| in_outbox(dir, std::path::Path::new(name));

        assert!(inside("/cache/piirit/webxdc/outbox/5/notes.txt"));
        // A name the app chose is kept whole, quirks and all.
        assert!(inside("/cache/piirit/webxdc/outbox/5/two words.mp4"));

        // Another app's outbox, which is another app's business.
        assert!(!inside("/cache/piirit/webxdc/outbox/6/notes.txt"));
        // The outbox itself, and the cache around it.
        assert!(!inside("/cache/piirit/webxdc/outbox/5"));
        assert!(!inside("/cache/piirit/webxdc/outbox"));
        // Below it rather than in it.
        assert!(!inside("/cache/piirit/webxdc/outbox/5/deeper/notes.txt"));
        // The reader's own files, however the path is written.
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
        // The core wraps nothing, but a blob that arrived wrapped is
        // still the blob.
        assert_eq!(decode_base64("aGVs\r\nbG8").as_deref(), Some(&b"hello"[..]));
        assert_eq!(decode_base64("not base64!"), None);
    }

    #[test]
    fn base64_round_trips_every_byte() {
        // Encoded by hand, so the decoder is checked against the
        // alphabet rather than against itself.
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
        // A head that carries attributes is still a head.
        assert!(inject("<HEAD lang=\"en\">a")
            .starts_with("<HEAD lang=\"en\"><script src=\"webxdc.js\"></script>"));
        // And a document without one gets the API first of all.
        assert!(inject("<p>hi</p>").starts_with("<script src=\"webxdc.js\"></script>"));
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
        // Every refusal carries its reason: a body with nothing in it is
        // a blank page, and that is what a broken app looks like too.
        let refused = Response::empty("404 Not Found");
        assert_eq!(refused.body, b"404 Not Found");

        // The app's own page is the one worth a sentence, and the core's
        // words go in it as text.
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
