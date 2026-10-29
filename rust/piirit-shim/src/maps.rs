//! Map: Delta Chat's maps app (`vendor/maps/`) as core's webxdc integration, as Desktop does.
//! Core feeds it last day's locations of one chat, or all, as status updates; app runs in
//! `WebxdcPage.qml`, tiles fetched by core (`webxdc_host.rs`).

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;
use serde_json::Value;

use crate::core::connection;
use crate::json;
use crate::qr::cache_file;

/// Unmodified upstream zip; pinned by `scripts/fetch-maps-xdc.sh`. Compiled in: packaged file
/// could go missing on phone only.
const MAPS_XDC: &[u8] = include_bytes!("../../../vendor/maps/maps.xdc");

/// Desktop commit `MAPS_XDC` came from. Core keeps it per profile: other value = older app.
pub(crate) const MAPS_VERSION: &str = "bb52bd1d61e8458749745ae93509cd380432c719";

/// Where core keeps `MAPS_VERSION`. Desktop's own: `ui.desktop.maps_version`.
const VERSION_KEY: &str = "ui.piirit.maps_version";

/// Opens map: readies app in core, says where to look.
///
/// ```qml
/// LocationMap { id: map; account_id: 1; onOpened: push(WebxdcPage, message_id, query) }
/// map.open(chatId, senderId, messageId)
/// ```
#[derive(QObject, Default)]
pub struct LocationMap {
    base: qt_base_class!(trait QObject),

    /// Profile.
    pub account_id: qt_property!(u32),

    /// Chat 0: every chat. Message with own point: map there; else sender's track, if any.
    pub open: qt_method!(fn(&mut self, chat_id: u32, contact_id: u32, message_id: u32)),

    /// App's message, and query for its page (`?poi=…`, `?contactId=…`, or "").
    pub opened: qt_signal!(message_id: u32, query: QString),
    /// Failed; message from core.
    pub error: qt_signal!(message: QString),

    /// `open` in flight: second tap would push second page.
    opening: bool,
}

impl LocationMap {
    /// Ready app for chat; answers on `opened`.
    pub fn open(&mut self, chat_id: u32, contact_id: u32, message_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || self.opening {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        self.opening = true;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(u32, String), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            this.borrow_mut().opening = false;
            match result {
                Ok((app, query)) => this.borrow().opened(app, query.into()),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let app = integration(&rpc, account_id, chat_id).await;
            let result = match app {
                Ok(app) => {
                    let point = if message_id == 0 {
                        None
                    } else {
                        // Unreadable: map opens, just not on point.
                        locations(&rpc, account_id, chat_id, contact_id)
                            .await
                            .ok()
                            .and_then(|found| own_point(&found, message_id))
                    };
                    Ok((app, start_query(point, contact_id)))
                }
                Err(err) => Err(err),
            };
            done(result);
        });
    }
}

/// App's message, set to `chat_id`. Missing or older app replaced first, as Desktop.
async fn integration(rpc: &RpcClient, account_id: u32, chat_id: u32) -> Result<u32, String> {
    let version: Option<String> = rpc
        .call("get_config", (account_id, VERSION_KEY))
        .await
        .map_err(|err| err.to_string())?;
    if version.as_deref() == Some(MAPS_VERSION) {
        if let Some(app) = init(rpc, account_id, chat_id).await? {
            return Ok(app);
        }
    }
    install(rpc, account_id).await?;
    init(rpc, account_id, chat_id)
        .await?
        .ok_or_else(|| "core did not take the map app".to_string())
}

/// Core's app, pointed at chat (0: all); `None` if core has none.
async fn init(rpc: &RpcClient, account_id: u32, chat_id: u32) -> Result<Option<u32>, String> {
    let chat = (chat_id != 0).then_some(chat_id);
    rpc.call("init_webxdc_integration", (account_id, chat))
        .await
        .map_err(|err| err.to_string())
}

/// Hand bundled app to core, which copies it; then note version.
async fn install(rpc: &RpcClient, account_id: u32) -> Result<(), String> {
    let path = cache_file(&format!("maps/{account_id}.xdc"))?;
    tokio::fs::write(&path, MAPS_XDC)
        .await
        .map_err(|err| format!("cannot write the map app: {err}"))?;
    let set: Result<Value, _> = rpc
        .call(
            "set_webxdc_integration",
            (account_id, path.to_string_lossy().as_ref()),
        )
        .await;
    // Copied or not, file no longer needed.
    tokio::fs::remove_file(&path).await.ok();
    set.map_err(|err| err.to_string())?;
    rpc.call::<_, Value>("set_config", (account_id, VERSION_KEY, MAPS_VERSION))
        .await
        .map(|_| ())
        .map_err(|err| err.to_string())
}

/// `get_locations` for sender in chat, newest first, whole history: app knows only last day.
async fn locations(
    rpc: &RpcClient,
    account_id: u32,
    chat_id: u32,
    contact_id: u32,
) -> Result<Vec<Value>, String> {
    // Params: account, chat, contact, from, to (0: now). Pinned: real_server.rs.
    rpc.call("get_locations", (account_id, chat_id, contact_id, 0, 0))
        .await
        .map_err(|err| err.to_string())
}

/// Message's own point, from core's `get_locations`. Live sharing's start notice has none.
pub(crate) fn own_point(locations: &[Value], message_id: u32) -> Option<(f64, f64)> {
    let own = locations
        .iter()
        .find(|location| json::u32_at(location, "msgId") == message_id)?;
    Some((
        own.get("latitude").and_then(Value::as_f64)?,
        own.get("longitude").and_then(Value::as_f64)?,
    ))
}

/// App's query: point if any (`contactId` would move map off it once tracks load), else
/// sender's track, else app's own start.
pub(crate) fn start_query(point: Option<(f64, f64)>, contact_id: u32) -> String {
    match point {
        Some((latitude, longitude)) => format!("?poi={latitude},{longitude}"),
        None if contact_id != 0 => format!("?contactId={contact_id}"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{own_point, start_query, MAPS_VERSION, MAPS_XDC};
    use serde_json::json;

    #[test]
    fn a_row_finds_its_own_point_and_nothing_else() {
        // As core: newest first, one sender streaming, one point sent once.
        let found = [
            json!({"latitude": 60.2, "longitude": 25.0, "contactId": 10, "msgId": 0}),
            json!({"latitude": 60.1, "longitude": 24.9, "contactId": 10, "msgId": 42}),
            json!({"latitude": 59.0, "longitude": 24.0, "contactId": 11, "msgId": 7}),
        ];
        assert_eq!(own_point(&found, 42), Some((60.1, 24.9)));
        assert_eq!(own_point(&found, 7), Some((59.0, 24.0)));
        // Start notice: no point of its own.
        assert_eq!(own_point(&found, 99), None);
        let broken = [json!({"latitude": 60.1, "contactId": 10, "msgId": 42})];
        assert_eq!(own_point(&broken, 42), None);
    }

    #[test]
    fn the_map_opens_on_the_point_else_on_the_sender() {
        assert_eq!(
            start_query(Some((60.169_52, 24.935_45)), 10),
            "?poi=60.16952,24.93545"
        );
        assert_eq!(start_query(Some((-33.5, -151.25)), 0), "?poi=-33.5,-151.25");
        assert_eq!(start_query(None, 10), "?contactId=10");
        assert_eq!(start_query(None, 0), "");
    }

    #[test]
    fn the_bundled_app_is_the_pinned_one() {
        // Fetch script and version core is told must name one commit: else phone keeps old app.
        let script = include_str!("../../../scripts/fetch-maps-xdc.sh");
        assert!(
            script.contains(&format!("COMMIT=\"{MAPS_VERSION}\"")),
            "fetch-maps-xdc.sh pins another commit than MAPS_VERSION"
        );
        assert!(MAPS_XDC.starts_with(b"PK\x03\x04"), "maps.xdc is no zip");
    }
}
