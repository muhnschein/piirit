//! A webxdc app: what the core knows about one, and running it.
//!
//! A webxdc is an app somebody sent: a zip with an index.html in it, which
//! everyone in the chat runs their own copy of and which keeps the copies
//! in step by sending status updates through the chat. The core does all
//! of that -- it decides a file is one, keeps its updates, and reads the
//! archive -- so what is left here is showing the app and running it.
//!
//! Showing it is [`extras`], which fills in the three fields a message row
//! draws: the app's name, what it currently says about itself, and its
//! icon, written once into the cache because QML draws a picture from a
//! path rather than from bytes.
//!
//! Running it is [`WebxdcApp`], which starts the loopback host in
//! `webxdc_host.rs` and hands QML the URL to point a `WebView` at. Closing
//! the page drops the object, which stops the host.

use std::sync::Arc;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;
use crate::json;
use crate::qr::cache_file;
use crate::webxdc_host::{decode_base64, Host, Instance};

/// The directory under the cache an app picked from the store waits in
/// until it has been sent.
const STORE_DIR: &str = "webxdc/store";

/// The directory under the cache the app icons live in.
const ICONS_DIR: &str = "webxdc";

/// What the core says about one webxdc message.
pub(crate) struct Info {
    /// The app's name, from its manifest or its file name.
    pub(crate) name: String,
    /// The icon's name *inside the archive*, which is not a path anything
    /// outside the core can open.
    pub(crate) icon: String,
    /// The document the app is editing, when it is that kind of app.
    pub(crate) document: String,
    /// The line the app puts under its own name -- "3 votes", a score --
    /// which it changes as it is played.
    pub(crate) summary: String,
    /// Where the app's source is, when its manifest says.
    pub(crate) source_code_url: String,
    /// What this account's updates are seen as coming from, inside the
    /// app. The core's own answer, which is not the account's address.
    pub(crate) self_addr: String,
    /// Milliseconds an app should leave between updates.
    pub(crate) send_update_interval: u64,
    /// The largest update the core will take, in bytes.
    pub(crate) send_update_max_size: u64,
}

/// What a message row shows for a webxdc.
#[derive(Default)]
pub(crate) struct Extras {
    /// The app's name.
    pub(crate) name: String,
    /// The document being edited, empty when it is not that kind of app.
    pub(crate) document: String,
    /// What the app says about itself now.
    pub(crate) summary: String,
    /// A path to the icon in the cache, empty when there is none to draw.
    pub(crate) icon_path: String,
}

/// What the core says about a webxdc message.
async fn fetch_info(rpc: &RpcClient, account_id: u32, message_id: u32) -> Result<Info, String> {
    let answer: serde_json::Value = rpc
        .call("get_webxdc_info", (account_id, message_id))
        .await
        .map_err(|err| err.to_string())?;
    Ok(Info {
        name: json::str_at(&answer, "name").to_string(),
        icon: json::str_at(&answer, "icon").to_string(),
        document: json::str_at(&answer, "document").to_string(),
        summary: json::str_at(&answer, "summary").to_string(),
        source_code_url: json::str_at(&answer, "sourceCodeUrl").to_string(),
        self_addr: json::str_at(&answer, "selfAddr").to_string(),
        // Absent from an older core, and 0 is what an app reads as "no
        // limit given" either way.
        send_update_interval: json::i64_at(&answer, "sendUpdateInterval")
            .try_into()
            .unwrap_or(0),
        send_update_max_size: json::i64_at(&answer, "sendUpdateMaxSize")
            .try_into()
            .unwrap_or(0),
    })
}

/// The app's icon as a file, written into the cache the first time it is
/// asked for. Empty when the app has none or the core would not give it.
///
/// The icon is inside the archive, so it has to come through the core and
/// be put somewhere an `Image` can load it. It never changes for a given
/// message, so the file is written once and found afterwards.
async fn fetch_icon(rpc: &RpcClient, account_id: u32, message_id: u32, icon: &str) -> String {
    if icon.is_empty() {
        return String::new();
    }
    // The name the file is written under is built from two numbers and
    // one of five literals: the icon is named inside an archive somebody
    // sent, and nothing of what they wrote reaches a path here. Qt reads
    // the picture itself either way, so the extension only has to be
    // plausible.
    let extension = match icon
        .rsplit_once('.')
        .map(|(_, tail)| tail.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => "jpg",
        Some("gif") => "gif",
        Some("webp") => "webp",
        Some("svg") => "svg",
        _ => "png",
    };
    let name = format!("{ICONS_DIR}/{account_id}-{message_id}.{extension}");
    let Ok(path) = cache_file(&name) else {
        return String::new();
    };
    if path.exists() {
        return path.to_string_lossy().into_owned();
    }
    let answer: Result<String, _> = rpc
        .call("get_webxdc_blob", (account_id, message_id, icon))
        .await;
    let Some(bytes) = answer.ok().as_deref().and_then(decode_base64) else {
        return String::new();
    };
    match std::fs::write(&path, bytes) {
        Ok(()) => path.to_string_lossy().into_owned(),
        Err(_) => String::new(),
    }
}

/// What a message row shows for a webxdc: its name, its state, its icon.
///
/// Everything empty when the core will not say, which draws as the file
/// row any other attachment gets rather than as a broken app.
pub(crate) async fn extras(rpc: &RpcClient, account_id: u32, message_id: u32) -> Extras {
    let Ok(info) = fetch_info(rpc, account_id, message_id).await else {
        return Extras::default();
    };
    let icon_path = fetch_icon(rpc, account_id, message_id, &info.icon).await;
    Extras {
        name: info.name,
        document: info.document,
        summary: info.summary,
        icon_path,
    }
}

/// One webxdc app: what it is called, and where it is running.
///
/// ```qml
/// WebxdcApp { id: app; account_id: page.accountId; message_id: page.messageId }
/// WebView { url: app.url }
/// ```
///
/// `start` puts the app on a loopback address of its own and answers on
/// `url`; the object stops serving when it goes away, so a page that
/// closes needs to do nothing else.
#[derive(QObject, Default)]
pub struct WebxdcApp {
    base: qt_base_class!(trait QObject),

    /// Which account the message belongs to. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY app_changed),
    /// The message the app arrived as. Setting it reloads.
    pub message_id: qt_property!(u32; WRITE set_message_id NOTIFY app_changed),
    /// Emitted when the account or the message changes.
    pub app_changed: qt_signal!(),

    /// The app's name.
    pub name: qt_property!(QString; NOTIFY loaded_changed),
    /// A path to the app's icon, empty when it has none.
    pub icon_path: qt_property!(QString; NOTIFY loaded_changed),
    /// The document the app is editing, empty when it edits none.
    pub document: qt_property!(QString; NOTIFY loaded_changed),
    /// What the app says about itself: "3 votes", a score, a state.
    pub summary: qt_property!(QString; NOTIFY loaded_changed),
    /// Where the app's source is, empty when its manifest does not say.
    pub source_code_url: qt_property!(QString; NOTIFY loaded_changed),
    /// True once a load has finished, however it went.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted after every load.
    pub loaded_changed: qt_signal!(),

    /// Where the app is being served, empty until `start` has answered.
    /// What a `WebView` is pointed at.
    pub url: qt_property!(QString; NOTIFY url_changed),
    /// Emitted when the app starts or stops being served.
    pub url_changed: qt_signal!(),

    /// Something failed. The message is the core's own.
    pub error: qt_signal!(message: QString),
    /// The message this app came in is gone, so there is nothing left to
    /// run: the page that is showing it should leave.
    pub gone: qt_signal!(),
    /// The app has handed something over: a file written into the cache,
    /// a piece of text, or both.
    ///
    /// Named for what happened rather than for the call that did it. The
    /// webxdc call is `sendToChat` and a chat is the only destination the
    /// API can name, but the button an app draws for it is a download and
    /// the reader means their phone by it -- so what to do with this is
    /// the page's question, and it asks. See `webxdc_host::HandedOver`.
    pub handed_over: qt_signal!(file_path: QString, text: QString),

    /// Reload what the core says about the app.
    pub reload: qt_method!(fn(&mut self)),
    /// Start serving the app. Answers on `url`.
    pub start: qt_method!(fn(&mut self)),
    /// Stop serving it. Called for you when the object goes away.
    pub stop: qt_method!(fn(&mut self)),
    /// Delete a file the app handed over, now that the page has saved
    /// it somewhere the reader keeps things.
    pub discard: qt_method!(fn(&mut self, file_path: QString)),
    /// Apply one core event. Only what changes this app is acted on.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// The loopback host, while the app is running.
    host: Option<Host>,
    /// True between `start` and `stop`. A host that finishes coming up
    /// after the page has closed is dropped rather than left serving an
    /// app nobody can see.
    wanted: bool,
    /// A `start` is in flight, so a second one would raise a second host
    /// and lose the first.
    starting: bool,
    /// Counts loads, so a slow answer to an older question cannot land on
    /// top of a newer one; see `ChatInfo`. Loads only: starting the app
    /// is not a load, and the two must not cancel each other.
    generation: u64,
}

impl WebxdcApp {
    /// Set the account and reload if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.app_changed();
            self.reload();
        }
    }

    /// Set the message and reload if it changed.
    pub fn set_message_id(&mut self, message_id: u32) {
        if self.message_id != message_id {
            // Whatever was being served was the old message's app.
            self.stop();
            self.message_id = message_id;
            self.loaded = false;
            self.loaded_changed();
            self.app_changed();
            self.reload();
        }
    }

    /// Reload what the core says about the app.
    pub fn reload(&mut self) {
        let (account_id, message_id) = (self.account_id, self.message_id);
        if account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(Info, String), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok((info, icon_path)) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.name = info.name.into();
                        this_mut.icon_path = icon_path.into();
                        this_mut.document = info.document.into();
                        this_mut.summary = info.summary.into();
                        this_mut.source_code_url = info.source_code_url.into();
                        this_mut.loaded = true;
                    }
                    this.borrow().loaded_changed();
                }
                Err(err) => {
                    // Loaded in the sense that matters: the wait is over.
                    this.borrow_mut().loaded = true;
                    this.borrow().loaded_changed();
                    this.borrow().error(err.into());
                }
            }
        });

        runtime.spawn(async move {
            let result = match fetch_info(&rpc, account_id, message_id).await {
                Ok(info) => {
                    let icon_path = fetch_icon(&rpc, account_id, message_id, &info.icon).await;
                    Ok((info, icon_path))
                }
                Err(err) => Err(err),
            };
            done(result);
        });
    }

    /// Start serving the app.
    pub fn start(&mut self) {
        let (account_id, message_id) = (self.account_id, self.message_id);
        if account_id == 0 || message_id == 0 || self.host.is_some() || self.starting {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        self.wanted = true;
        self.starting = true;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        // What the host does with a file the app hands over. Queued, so
        // a request that arrives on the host's own thread is raised on
        // the Qt one, where the page that answers it lives.
        let to_page: QPointer<Self> = QPointer::from(&*self);
        let raise = queued_callback(move |(path, text): (String, String)| {
            if let Some(this) = to_page.as_pinned() {
                this.borrow().handed_over(path.into(), text.into());
            }
        });
        let to_chat: crate::webxdc_host::HandedOver =
            std::sync::Arc::new(move |path, text| raise((path, text)));

        let done = queued_callback(move |result: Result<Host, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            this.borrow_mut().starting = false;
            match result {
                // The page was closed while the host was coming up:
                // dropping it here is what stops it again.
                Ok(_) if !this.borrow().wanted => {}
                Ok(host) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.url = host.url().into();
                        this_mut.host = Some(host);
                    }
                    this.borrow().url_changed();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            done(host_for(&rpc, account_id, message_id, to_chat).await);
        });
    }

    /// Forget a file the app handed over, now that it has been kept.
    ///
    /// The page saves a copy and this deletes the original, which is in
    /// the cache and has nothing left to do. Only a file in this app's
    /// own outbox goes; see `webxdc_host::discard_outgoing`.
    pub fn discard(&mut self, file_path: QString) {
        crate::webxdc_host::discard_outgoing(self.message_id, &file_path.to_string());
    }

    /// Stop serving the app.
    pub fn stop(&mut self) {
        self.wanted = false;
        // Dropping the host is what stops it; see `webxdc_host::Host`.
        if self.host.take().is_some() {
            self.url = QString::default();
            self.url_changed();
        }
    }

    /// Apply one core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || self.account_id == 0 {
            return;
        }
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        if json::u32_opt(&payload, "msgId") != Some(self.message_id) {
            return;
        }
        match kind.to_string().as_str() {
            // Deleted here or on another device. There is nothing left to
            // serve, and the core would answer no blob for it.
            "WebxdcInstanceDeleted" => {
                self.stop();
                self.gone();
            }
            // The app's own summary follows what the chat did with it,
            // which is what the page's header shows.
            "WebxdcStatusUpdate" => self.reload(),
            _ => {}
        }
    }
}

/// Picking an app from the store.
///
/// The store is a website, and choosing an app there is downloading a
/// .xdc. The page itself is the `WebView`'s to load; this is the other
/// half -- fetching the file the reader tapped, which goes through the
/// core rather than the engine, so what lands is a file this app can
/// hand to a chat rather than a download somewhere in the browser's
/// world. It is how deltachat-android does it too.
///
/// ```qml
/// WebxdcStore { id: store; account_id: page.accountId }
/// // store.fetch(url) -> onPicked: attach(path)
/// ```
#[derive(QObject, Default)]
pub struct WebxdcStore {
    base: qt_base_class!(trait QObject),

    /// Which account does the fetching. The core fetches per account,
    /// through whatever it has been told to reach the network with.
    pub account_id: qt_property!(u32),

    /// True while a download is in flight. One at a time: the reader
    /// tapped one app.
    pub fetching: qt_property!(bool; NOTIFY fetching_changed),
    /// Emitted when a download starts or ends.
    pub fetching_changed: qt_signal!(),

    /// Fetch the .xdc at `url` and put it on the phone. Answers on
    /// `picked`.
    pub fetch: qt_method!(fn(&mut self, url: QString)),

    /// The app is on the phone at this path, ready to be sent.
    pub picked: qt_signal!(path: QString),
    /// Something failed. The message is the core's own.
    pub error: qt_signal!(message: QString),
}

impl WebxdcStore {
    /// Fetch the app at `url`.
    pub fn fetch(&mut self, url: QString) {
        let account_id = self.account_id;
        if account_id == 0 || self.fetching {
            return;
        }
        let url = url.to_string();
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        self.fetching = true;
        self.fetching_changed();

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<String, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            this.borrow_mut().fetching = false;
            this.borrow().fetching_changed();
            match result {
                Ok(path) => this.borrow().picked(path.into()),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            done(download(&rpc, account_id, &url).await);
        });
    }
}

/// One app off the store, through the core and onto the phone.
async fn download(rpc: &RpcClient, account_id: u32, url: &str) -> Result<String, String> {
    let answer: serde_json::Value = rpc
        .call("get_http_response", (account_id, url))
        .await
        .map_err(|err| err.to_string())?;
    let bytes = decode_base64(json::str_at(&answer, "blob"))
        .ok_or_else(|| "the app came back as something unreadable".to_string())?;
    if bytes.is_empty() {
        return Err("the app came back empty".to_string());
    }
    let path = cache_file(&format!("{STORE_DIR}/{}", app_file_name(url)))?;
    std::fs::write(&path, bytes).map_err(|err| format!("cannot save the app: {err}"))?;
    Ok(path.to_string_lossy().into_owned())
}

/// What to call a downloaded app: the last part of its URL, kept to what
/// a file may be named, and always a .xdc.
///
/// The name is the one the other end of a chat sees, so it is worth
/// keeping -- and it comes off a web page, so none of it reaches a path
/// unfiltered.
fn app_file_name(url: &str) -> String {
    let name: String = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .take(48)
        .collect();
    let stem = name.strip_suffix(".xdc").unwrap_or(&name).trim_matches('.');
    if stem.is_empty() {
        "app.xdc".to_string()
    } else {
        format!("{stem}.xdc")
    }
}

/// Everything the host needs about one app, gathered and started.
async fn host_for(
    rpc: &Arc<RpcClient>,
    account_id: u32,
    message_id: u32,
    to_chat: crate::webxdc_host::HandedOver,
) -> Result<Host, String> {
    let info = fetch_info(rpc, account_id, message_id).await?;
    // The name this account goes by, which the app shows beside whatever
    // it hears from this end. The core has no self-name of its own for a
    // webxdc, so it is the profile's, and its address when it has none.
    let display_name: String = rpc
        .call::<_, Option<String>>("get_config", (account_id, "displayname"))
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let self_name = if display_name.is_empty() {
        info.self_addr.clone()
    } else {
        display_name
    };
    crate::webxdc_host::start(
        Arc::clone(rpc),
        Instance {
            account_id,
            message_id,
            self_addr: info.self_addr,
            self_name,
            send_update_interval: info.send_update_interval,
            send_update_max_size: info.send_update_max_size,
        },
        to_chat,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::app_file_name;

    #[test]
    fn an_app_keeps_the_name_the_store_gave_it_and_nothing_else() {
        assert_eq!(
            app_file_name("https://webxdc.org/apps/checkers.xdc"),
            "checkers.xdc"
        );
        assert_eq!(
            app_file_name("https://webxdc.org/apps/checkers.xdc?v=2#top"),
            "checkers.xdc"
        );
        // Anything that is not a file name is not kept: what is written
        // here is two literals and what survives the filter.
        assert_eq!(
            app_file_name("https://example.org/../../etc/passwd"),
            "passwd.xdc"
        );
        assert_eq!(
            app_file_name("https://example.org/%2e%2e%2fboom"),
            "2e2e2fboom.xdc"
        );
        assert_eq!(app_file_name("https://example.org/"), "app.xdc");
        assert_eq!(app_file_name("https://example.org/..."), "app.xdc");
        // A name longer than a name is cut rather than refused.
        let long = format!("https://example.org/{}.xdc", "a".repeat(200));
        assert!(app_file_name(&long).len() <= 52);
    }
}
