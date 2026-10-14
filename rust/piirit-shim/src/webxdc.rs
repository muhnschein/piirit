//! Webxdc app: core's info on one, and running it.
//!
//! Webxdc: sent zip with index.html; each chat member runs own copy, synced by status updates.
//! Core does detection, updates, archive. Here: show and run.
//!
//! [`extras`] fills message row (name, summary, icon); icon cached once (QML loads by path).
//! [`WebxdcApp`] runs loopback host (`webxdc_host.rs`) for `WebView`; page close drops it, stops
//! host.

use std::sync::Arc;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;
use crate::json;
use crate::qr::cache_file;
use crate::webxdc_host::{decode_base64, Host, Instance};

/// Cache dir for picked apps awaiting send.
const STORE_DIR: &str = "webxdc/store";

/// Cache dir for app icons.
const ICONS_DIR: &str = "webxdc";

/// Core's info on one webxdc message.
pub(crate) struct Info {
    /// App name, from manifest or file name.
    pub(crate) name: String,
    /// Icon name inside archive, not a path.
    pub(crate) icon: String,
    /// Document app edits.
    pub(crate) document: String,
    /// Line under app name ("3 votes"), changes as played.
    pub(crate) summary: String,
    /// Source location, if manifest says.
    pub(crate) source_code_url: String,
    /// Name updates show from; core's answer, not account address.
    pub(crate) self_addr: String,
    /// Ms between updates.
    pub(crate) send_update_interval: u64,
    /// Largest update core takes, bytes.
    pub(crate) send_update_max_size: u64,
}

/// What a message row shows for a webxdc.
#[derive(Default)]
pub(crate) struct Extras {
    /// App name.
    pub(crate) name: String,
    /// Document being edited; empty if none.
    pub(crate) document: String,
    /// Status line.
    pub(crate) summary: String,
    /// Icon path in cache; empty if none.
    pub(crate) icon_path: String,
}

/// What core says about a webxdc message.
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
        // Absent from older core; 0 reads as "no limit".
        send_update_interval: json::i64_at(&answer, "sendUpdateInterval")
            .try_into()
            .unwrap_or(0),
        send_update_max_size: json::i64_at(&answer, "sendUpdateMaxSize")
            .try_into()
            .unwrap_or(0),
    })
}

/// Icon as file in cache, written on first ask. Empty if none.
///
/// Written once per message.
async fn fetch_icon(rpc: &RpcClient, account_id: u32, message_id: u32, icon: &str) -> String {
    if icon.is_empty() {
        return String::new();
    }
    // Name: two numbers, one of five literals. Archive name never reaches path.
    // Qt picks image by content: extension only needs plausible.
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
    match tokio::fs::write(&path, bytes).await {
        Ok(()) => path.to_string_lossy().into_owned(),
        Err(_) => String::new(),
    }
}

/// Message row for webxdc: name, state, icon. Empty if core refuses: drawn as plain file row,
/// not broken app.
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

/// One webxdc app: name and where it runs.
///
/// ```qml
/// WebxdcApp { id: app; account_id: page.accountId; message_id: page.messageId }
/// WebView { url: app.url }
/// ```
///
/// `start` serves on own loopback address; answers on `url`. Drop stops serving, so closing
/// page needs nothing else.
#[derive(QObject, Default)]
pub struct WebxdcApp {
    base: qt_base_class!(trait QObject),

    /// Account of message. Setting reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY app_changed),
    /// Message. Setting reloads.
    pub message_id: qt_property!(u32; WRITE set_message_id NOTIFY app_changed),
    /// Emitted on account or message change.
    pub app_changed: qt_signal!(),

    /// App name.
    pub name: qt_property!(QString; NOTIFY loaded_changed),
    /// Icon path, empty if none.
    pub icon_path: qt_property!(QString; NOTIFY loaded_changed),
    /// Document edited, empty if none.
    pub document: qt_property!(QString; NOTIFY loaded_changed),
    /// Status ("3 votes"). As of last load; updates don't reload it.
    pub summary: qt_property!(QString; NOTIFY loaded_changed),
    /// Source location, empty if none.
    pub source_code_url: qt_property!(QString; NOTIFY loaded_changed),
    /// True once load finished.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted after every load.
    pub loaded_changed: qt_signal!(),

    /// Serving URL, empty until `start` answers.
    /// What `WebView` points at.
    pub url: qt_property!(QString; NOTIFY url_changed),
    /// Emitted when serving starts or stops.
    pub url_changed: qt_signal!(),

    /// Something failed; core's message.
    pub error: qt_signal!(message: QString),
    /// Message gone: page should leave.
    pub gone: qt_signal!(),
    /// App handed over: file, text, or both.
    ///
    /// Named for what happened, not the call: webxdc `sendToChat` can only name a chat, but app's
    /// button is a download meaning the phone. Page asks reader. See `webxdc_host::HandedOver`.
    pub handed_over: qt_signal!(file_path: QString, text: QString),

    /// Reload core's info.
    pub reload: qt_method!(fn(&mut self)),
    /// Start serving. Answers on `url`.
    pub start: qt_method!(fn(&mut self)),
    /// Stop serving. Called on drop.
    pub stop: qt_method!(fn(&mut self)),
    /// Delete handed-over file once page saved it.
    pub discard: qt_method!(fn(&mut self, file_path: QString)),
    /// Apply core event. Only what changes this app.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Loopback host, while running.
    host: Option<Host>,
    /// True between `start` and `stop`. Host ready after page closed is dropped.
    wanted: bool,
    /// `start` in flight: second would lose first host.
    starting: bool,
    /// Counts loads: stale answer can't land on newer one; see `ChatInfo`. Loads only: `start`
    /// isn't one; they must not cancel each other.
    generation: u64,
}

impl WebxdcApp {
    /// Set account; reload if changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.app_changed();
            self.reload();
        }
    }

    /// Set message; reload if changed.
    pub fn set_message_id(&mut self, message_id: u32) {
        if self.message_id != message_id {
            // Served app was old message's.
            self.stop();
            self.message_id = message_id;
            self.loaded = false;
            self.loaded_changed();
            self.app_changed();
            self.reload();
        }
    }

    /// Reload core's info.
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
                    // Loaded: wait over.
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

    /// Start serving.
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
        // Queued: request on host's thread is raised on Qt thread.
        let to_page: QPointer<Self> = QPointer::from(&*self);
        let raise = queued_callback(move |(path, text): (String, String)| {
            if let Some(this) = to_page.as_pinned() {
                this.borrow().handed_over(path.into(), text.into());
            }
        });
        let to_chat: crate::webxdc_host::HandedOver =
            Arc::new(move |path, text| raise((path, text)));

        let done = queued_callback(move |result: Result<Host, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            this.borrow_mut().starting = false;
            match result {
                // Page closed mid-start: dropping stops it.
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

    /// Forget handed-over file once kept.
    ///
    /// Page saved a copy; this deletes cache original. This app's outbox only; see
    /// `webxdc_host::discard_outgoing`.
    pub fn discard(&mut self, file_path: QString) {
        crate::webxdc_host::discard_outgoing(self.message_id, &file_path.to_string());
    }

    /// Stop serving.
    pub fn stop(&mut self) {
        self.wanted = false;
        // Dropping host stops it; see `webxdc_host::Host`.
        if self.host.take().is_some() {
            self.url = QString::default();
            self.url_changed();
        }
    }

    /// Apply core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || self.account_id == 0 {
            return;
        }
        let kind = kind.to_string();
        // Core drops events it can't queue: update may be lost, so request asks again.
        if kind == "EventChannelOverflow" {
            self.poke();
            return;
        }
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        if json::u32_opt(&payload, "msgId") != Some(self.message_id) {
            return;
        }
        match kind.as_str() {
            // Deleted here or on other device: nothing to serve; core has no blob.
            "WebxdcInstanceDeleted" => {
                self.stop();
                self.gone();
            }
            // New update from chat: wake app's waiting request. No reload: name unchanged.
            "WebxdcStatusUpdate" => self.poke(),
            _ => {}
        }
    }

    /// Wake waiting update request, if running.
    fn poke(&self) {
        if let Some(host) = &self.host {
            host.poke();
        }
    }
}

/// Picking app from store (a website; page is `WebView`'s): downloads tapped .xdc via core, not
/// engine, so it lands as a file to send. As deltachat-android does.
/// ```qml
/// WebxdcStore { id: store; account_id: page.accountId }
/// // store.fetch(url) -> onPicked: attach(path)
/// ```
#[derive(QObject, Default)]
pub struct WebxdcStore {
    base: qt_base_class!(trait QObject),

    /// Account for fetch.
    pub account_id: qt_property!(u32),

    /// True while downloading. One at a time.
    pub fetching: qt_property!(bool; NOTIFY fetching_changed),
    /// Emitted when download starts or ends.
    pub fetching_changed: qt_signal!(),

    /// Fetch .xdc at `url` onto phone. Answers on `picked`.
    pub fetch: qt_method!(fn(&mut self, url: QString)),

    /// App file path, ready to send.
    pub picked: qt_signal!(path: QString),
    /// Something failed; core's message.
    pub error: qt_signal!(message: QString),
}

impl WebxdcStore {
    /// Fetch app at `url`.
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

/// One app off store: via core, onto phone.
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
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|err| format!("cannot save the app: {err}"))?;
    Ok(path.to_string_lossy().into_owned())
}

/// File name for downloaded app: URL's last part, filtered, always .xdc.
///
/// Filtered: web page's name never reaches a path raw.
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

/// Everything host needs for one app, started.
async fn host_for(
    rpc: &Arc<RpcClient>,
    account_id: u32,
    message_id: u32,
    to_chat: crate::webxdc_host::HandedOver,
) -> Result<Host, String> {
    let info = fetch_info(rpc, account_id, message_id).await?;
    // Name app shows beside its updates: profile name, else address.
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
        // Not a file name: not kept.
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
        // Long name cut, not refused.
        let long = format!("https://example.org/{}.xdc", "a".repeat(200));
        assert!(app_file_name(&long).len() <= 52);
    }
}
