//! Locations: where a row's location is, map addresses for it, phone's position fed to core
//! while a chat gets it live.
//!
//! Map = OpenStreetMap's embed page in a `WebView` (`LocationPage.qml`); Delta Chat's maps app is
//! #255.

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;
use serde_json::Value;

use crate::core::connection;
use crate::json;

/// Half the map's height, degrees latitude: about 330 m, a few streets either side.
const HALF_SPAN: f64 = 0.003;

/// Place shown: one message's own point, or a sender's newest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Spot {
    pub latitude: f64,
    pub longitude: f64,
    /// Metres; 0 if sender gave none.
    pub accuracy: f64,
    /// Unix seconds.
    pub timestamp: i64,
    /// Sender's newest, not message's own: moves while they share.
    pub live: bool,
}

impl Spot {
    fn from_core(location: &Value, live: bool) -> Option<Self> {
        Some(Self {
            latitude: location.get("latitude").and_then(Value::as_f64)?,
            longitude: location.get("longitude").and_then(Value::as_f64)?,
            accuracy: location
                .get("accuracy")
                .and_then(Value::as_f64)
                .unwrap_or_default(),
            timestamp: json::i64_at(location, "timestamp"),
            live,
        })
    }
}

/// From core's `get_locations` (newest first): `message_id`'s own point, else `contact_id`'s
/// newest (live sharing's start notice has none of its own).
pub(crate) fn pick(locations: &[Value], message_id: u32, contact_id: u32) -> Option<Spot> {
    if let Some(own) = locations
        .iter()
        .find(|location| message_id != 0 && json::u32_at(location, "msgId") == message_id)
    {
        return Spot::from_core(own, false);
    }
    locations
        .iter()
        .find(|location| json::u32_at(location, "contactId") == contact_id)
        .and_then(|location| Spot::from_core(location, true))
}

/// osm.org with a pin: one-off message's text, so every client shows a link; browser offer.
pub(crate) fn osm_link(latitude: f64, longitude: f64) -> String {
    format!(
        "https://www.openstreetmap.org/?mlat={latitude:.5}&mlon={longitude:.5}\
         #map=16/{latitude:.5}/{longitude:.5}"
    )
}

/// osm.org's embed page: map and pin only, none of the full site's script.
pub(crate) fn map_url(latitude: f64, longitude: f64) -> String {
    // Square on the ground: degree of longitude shrinks towards poles; pole itself unreachable.
    let shrink = latitude.clamp(-85.0, 85.0).to_radians().cos();
    let half_width = HALF_SPAN / shrink;
    let (west, east) = (longitude - half_width, longitude + half_width);
    let (south, north) = (latitude - HALF_SPAN, latitude + HALF_SPAN);
    format!(
        "https://www.openstreetmap.org/export/embed.html\
         ?bbox={west:.5}%2C{south:.5}%2C{east:.5}%2C{north:.5}\
         &layer=mapnik&marker={latitude:.5}%2C{longitude:.5}"
    )
}

/// One row's location, as map page shows it.
///
/// ```qml
/// SharedLocation { account_id: 1; chat_id: 7; contact_id: 10; message_id: 42 }
/// WebView { url: place.map_url }
/// ```
#[derive(QObject, Default)]
pub struct SharedLocation {
    base: qt_base_class!(trait QObject),

    /// Account. Set reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY source_changed),
    /// Chat the row is in. Set reloads.
    pub chat_id: qt_property!(u32; WRITE set_chat_id NOTIFY source_changed),
    /// Row's sender; their newest location if row has none of its own. Set reloads.
    pub contact_id: qt_property!(u32; WRITE set_contact_id NOTIFY source_changed),
    /// Row; 0 for sender's newest. Set reloads.
    pub message_id: qt_property!(u32; WRITE set_message_id NOTIFY source_changed),
    /// Emitted when any of the four changes.
    pub source_changed: qt_signal!(),

    /// Core answered at least once.
    pub loaded: qt_property!(bool; NOTIFY place_changed),
    /// Core has a place for row.
    pub found: qt_property!(bool; NOTIFY place_changed),
    /// Sender's newest, refreshed as they move.
    pub live: qt_property!(bool; NOTIFY place_changed),
    /// Degrees.
    pub latitude: qt_property!(f64; NOTIFY place_changed),
    /// Degrees.
    pub longitude: qt_property!(f64; NOTIFY place_changed),
    /// Metres; 0 unknown.
    pub accuracy: qt_property!(f64; NOTIFY place_changed),
    /// Unix seconds, when sender was there.
    pub timestamp: qt_property!(i64; NOTIFY place_changed),
    /// Embed page; empty until found.
    pub map_url: qt_property!(QString; NOTIFY place_changed),
    /// Full osm.org page, for browser; empty until found.
    pub link: qt_property!(QString; NOTIFY place_changed),
    /// Emitted on each answer.
    pub place_changed: qt_signal!(),

    /// Failed; message from core.
    pub error: qt_signal!(message: QString),

    /// Ask again.
    pub reload: qt_method!(fn(&mut self)),
    /// Core event (`DeltaChatCore::core_event`).
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Stale answers dropped; see `ChatInfo`.
    generation: u64,
}

impl SharedLocation {
    /// Set account; reload on change.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set chat; reload on change.
    pub fn set_chat_id(&mut self, chat_id: u32) {
        if self.chat_id != chat_id {
            self.chat_id = chat_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set sender; reload on change.
    pub fn set_contact_id(&mut self, contact_id: u32) {
        if self.contact_id != contact_id {
            self.contact_id = contact_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set row; reload on change.
    pub fn set_message_id(&mut self, message_id: u32) {
        if self.message_id != message_id {
            self.message_id = message_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Ask core for chat's locations from sender.
    pub fn reload(&mut self) {
        let (account_id, chat_id, contact_id, message_id) = (
            self.account_id,
            self.chat_id,
            self.contact_id,
            self.message_id,
        );
        if account_id == 0 || chat_id == 0 || contact_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Option<Spot>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok(spot) => this.borrow_mut().show(spot),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            done(
                locations(&rpc, account_id, chat_id, contact_id)
                    .await
                    .map(|found| pick(&found, message_id, contact_id)),
            );
        });
    }

    /// Take core's answer. Nothing found keeps last place: sender's history deleted mid-view.
    fn show(&mut self, spot: Option<Spot>) {
        self.loaded = true;
        if let Some(spot) = spot {
            self.found = true;
            self.live = spot.live;
            self.latitude = spot.latitude;
            self.longitude = spot.longitude;
            self.accuracy = spot.accuracy;
            self.timestamp = spot.timestamp;
            self.map_url = map_url(spot.latitude, spot.longitude).into();
            self.link = osm_link(spot.latitude, spot.longitude).into();
        }
        self.place_changed();
    }

    /// Sender moved (or any contact: core may not say whose).
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || kind.to_string() != "LocationChanged" {
            return;
        }
        let payload: Value = serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        match json::u32_opt(&payload, "contactId") {
            Some(contact) if contact != self.contact_id => {}
            _ => self.reload(),
        }
    }
}

/// `get_locations` for one sender in one chat, newest first, whole history.
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

/// Some chat, any profile, gets reader's location live: phone's position goes to core.
///
/// ```qml
/// LiveLocation { id: live }
/// PositionSource { active: live.active; onPositionChanged: live.report(...) }
/// ```
#[derive(QObject, Default)]
pub struct LiveLocation {
    base: qt_base_class!(trait QObject),

    /// Core wants positions.
    pub active: qt_property!(bool; NOTIFY active_changed),
    /// Emitted on `active` change.
    pub active_changed: qt_signal!(),

    /// Ask core whether any chat gets location.
    pub refresh: qt_method!(fn(&mut self)),
    /// Phone's position, degrees and metres; core stores it for each sharing chat.
    pub report: qt_method!(fn(&mut self, latitude: f64, longitude: f64, accuracy: f64)),
    /// Core event (`DeltaChatCore::core_event`).
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Stale answers dropped; see `ChatInfo`.
    generation: u64,
}

impl LiveLocation {
    /// Ask every profile.
    pub fn refresh(&mut self) {
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |sharing: Option<bool>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            // Unreadable: leave as is, next event asks again.
            if let Some(sharing) = sharing {
                this.borrow_mut().set_active(sharing);
            }
        });
        runtime.spawn(async move {
            done(any_sharing(&rpc).await);
        });
    }

    /// Hand position to core; it says whether any chat still wants one (time up: no).
    pub fn report(&mut self, latitude: f64, longitude: f64, accuracy: f64) {
        if !self.active {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |wanted: Option<bool>| {
            let Some(this) = ptr.as_pinned() else { return };
            if wanted == Some(false) {
                this.borrow_mut().set_active(false);
            }
        });
        runtime.spawn(async move {
            done(
                rpc.call::<_, bool>("set_location", (latitude, longitude, accuracy))
                    .await
                    .ok(),
            );
        });
    }

    fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.active_changed();
        }
    }

    /// Sharing started, stopped or ran out: core says chat modified.
    pub fn handle_event(&mut self, _context_id: u32, kind: QString, _payload_json: QString) {
        if kind.to_string() == "ChatModified" {
            self.refresh();
        }
    }
}

/// Any profile sharing; `None` if core can't say.
async fn any_sharing(rpc: &RpcClient) -> Option<bool> {
    let accounts: Vec<u32> = rpc.call_unit("get_all_account_ids").await.ok()?;
    for account_id in accounts {
        if rpc
            .call::<_, bool>("is_sending_locations", (account_id,))
            .await
            .ok()?
        {
            return Some(true);
        }
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::{map_url, osm_link, pick, Spot};
    use serde_json::json;

    fn spot(latitude: f64, longitude: f64, timestamp: i64, live: bool) -> Spot {
        Spot {
            latitude,
            longitude,
            accuracy: 0.0,
            timestamp,
            live,
        }
    }

    #[test]
    fn a_row_shows_its_own_point_and_a_notice_its_senders_newest() {
        // As core: newest first, one sender streaming, one point sent once.
        let found = [
            json!({"latitude": 60.2, "longitude": 25.0, "timestamp": 300,
                   "contactId": 10, "msgId": 0}),
            json!({"latitude": 60.1, "longitude": 24.9, "timestamp": 200,
                   "contactId": 10, "msgId": 42}),
            json!({"latitude": 59.0, "longitude": 24.0, "timestamp": 100,
                   "contactId": 11, "msgId": 7}),
        ];
        assert_eq!(pick(&found, 42, 10), Some(spot(60.1, 24.9, 200, false)));
        // Start notice: no point of its own.
        assert_eq!(pick(&found, 99, 10), Some(spot(60.2, 25.0, 300, true)));
        assert_eq!(pick(&found, 0, 11), Some(spot(59.0, 24.0, 100, true)));
        assert_eq!(pick(&found, 99, 12), None);
        // Message 0 is no message: never matches a point without one.
        assert_eq!(pick(&found, 0, 10), Some(spot(60.2, 25.0, 300, true)));
    }

    #[test]
    fn a_point_without_coordinates_is_no_point() {
        let found = [json!({"timestamp": 300, "contactId": 10, "msgId": 42})];
        assert_eq!(pick(&found, 42, 10), None);
    }

    #[test]
    fn the_link_pins_the_place_to_a_metre() {
        assert_eq!(
            osm_link(60.169_524, 24.935_456),
            "https://www.openstreetmap.org/?mlat=60.16952&mlon=24.93546\
             #map=16/60.16952/24.93546"
        );
        assert_eq!(
            osm_link(-33.856_8, -151.215_3),
            "https://www.openstreetmap.org/?mlat=-33.85680&mlon=-151.21530\
             #map=16/-33.85680/-151.21530"
        );
    }

    #[test]
    fn the_map_is_square_on_the_ground_around_the_pin() {
        // At 60° a degree of longitude is half one of latitude: box twice as wide in degrees.
        assert_eq!(
            map_url(60.0, 25.0),
            "https://www.openstreetmap.org/export/embed.html\
             ?bbox=24.99400%2C59.99700%2C25.00600%2C60.00300\
             &layer=mapnik&marker=60.00000%2C25.00000"
        );
        assert_eq!(
            map_url(0.0, 0.0),
            "https://www.openstreetmap.org/export/embed.html\
             ?bbox=-0.00300%2C-0.00300%2C0.00300%2C0.00300\
             &layer=mapnik&marker=0.00000%2C0.00000"
        );
    }

    #[test]
    fn the_map_near_a_pole_stays_a_few_streets_wide() {
        // Widest at 85°: 0.003 / cos 85°.
        assert!(
            map_url(90.0, 0.0).contains("?bbox=-0.03442%2C89.99700%2C0.03442%2C90.00300&"),
            "box at a pole not that of 85°"
        );
    }
}
