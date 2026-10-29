//! Locations: link a one-off point carries, phone's position fed to core while a chat gets it
//! live. Map: `maps.rs`.

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;

/// osm.org with a pin: one-off message's text, so every client shows a link; browser offer.
pub(crate) fn osm_link(latitude: f64, longitude: f64) -> String {
    format!(
        "https://www.openstreetmap.org/?mlat={latitude:.5}&mlon={longitude:.5}\
         #map=16/{latitude:.5}/{longitude:.5}"
    )
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
    use super::osm_link;

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
}
