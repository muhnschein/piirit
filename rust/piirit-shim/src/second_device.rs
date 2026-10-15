//! Offer one profile to a second device.
//!
//! Other half of takeover (`signup.rs`): core backup transfer over LAN; this device offers.
//!
//! `provide_backup` runs until taken or stopped. `get_backup_qr` blocks until provider ready,
//! gives up after a minute; two calls on one account fine (correlated by id). Page-owned, as
//! `backup.rs`.
//!
//! Core pauses account IO while offer up, resumes after; not restarted here (races core);
//! page says so.
//!
//! `ImexProgress` only proof of success (none until device connects: code until first event,
//! bar after; heard via `handle_event` as `backup.rs`). 1000 taken, 0 failed. Provider `Ok` is
//! no proof (bundled core says `Ok` after stop too): `taken` fires on 1000. Offer ending otherwise,
//! unstopped by reader = `stalled` (app's word). Reader stop ends at once; late answer ignored.
//!
//! Race: progress polled separately, may land after `Ok`, so `Ok` waits [`SETTLE`], then
//! stall. Wait only on offers that went nowhere; refusals passed at once.

use std::time::Duration;

use qmetaobject::*;

use crate::core::connection;
use crate::json;

/// Permille counted up to when done.
const DONE: u32 = 1000;

/// Grace for progress after `Ok`: event behind poll on loaded phone, reader still on page.
const SETTLE: Duration = Duration::from_secs(1);

/// One profile offered to a device.
///
/// ```qml
/// SecondDevice { id: device; account_id: page.accountId }
/// QrCode { text: device.code }
/// Button { onClicked: device.offer() }
/// ```
#[derive(QObject, Default)]
pub struct SecondDevice {
    base: qt_base_class!(trait QObject),

    /// Profile offered; set once by owning page.
    pub account_id: qt_property!(u32),

    /// True from `offer` until provider ends.
    pub running: qt_property!(bool; NOTIFY running_changed),
    /// Emitted when [`Self::running`] changes.
    pub running_changed: qt_signal!(),

    /// Code for other device; empty outside offer. `DCBACKUP` payload (digit = transfer
    /// version), opaque here. Also shown as text for cameras that won't read QR.
    pub code: qt_property!(QString; NOTIFY code_changed),
    /// Emitted when [`Self::code`] changes.
    pub code_changed: qt_signal!(),

    /// Transfer progress, permille. 0 while code waits unread.
    pub permille: qt_property!(u32; NOTIFY permille_changed),
    /// Emitted when [`Self::permille`] changes.
    pub permille_changed: qt_signal!(),

    /// Offer profile. Answers `taken`, `stalled` or `error`; code goes on [`Self::code`].
    pub offer: qt_method!(fn(&mut self)),

    /// Stop running offer; provider's later answer: no outcome.
    pub cancel: qt_method!(fn(&mut self)),

    /// Feed `core_event`; others ignored, no page filter needed.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// A device read the code and has the profile now.
    pub taken: qt_signal!(),

    /// Ended untaken, unstopped. App's own reason.
    pub stalled: qt_signal!(),

    /// Nothing offered, core's words.
    pub error: qt_signal!(message: QString),

    /// Offer counter: late provider answers land nowhere. Not a flag: stopped provider answers
    /// `SETTLE` later, maybe after new offer, and once ended it with provider unstoppable.
    generation: u64,
}

impl SecondDevice {
    /// Offer this profile to a device that reads the code.
    pub fn offer(&mut self) {
        // Core refuses second provider per account: would read as this offer failing.
        if self.running {
            return;
        }
        let account_id = self.account_id;
        if account_id == 0 {
            self.error(QString::from("no profile to offer"));
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.set_permille(0);
        self.set_code(QString::default());
        self.set_running(true);

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let ended = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Offer already over: late answer not news.
            if this.borrow().generation != generation {
                return;
            }
            this.borrow_mut().end();
            match result {
                // Finished without transfer: nobody came, or left.
                Ok(()) => this.borrow().stalled(),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let showing = queued_callback(move |result: Result<String, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Provider may end before code (refused, stopped): late code dropped.
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok(code) => this.borrow_mut().set_code(code.into()),
                // No code: stop provider, don't also report its refusal.
                Err(err) => {
                    this.borrow_mut().stop();
                    this.borrow().error(err.into());
                }
            }
        });

        let provider = rpc.clone();
        runtime.spawn(async move {
            let result = provider
                .call::<_, ()>("provide_backup", (account_id,))
                .await
                .map_err(|err| err.to_string());
            // `Ok`: wait for progress (module doc); refusal at once.
            if result.is_ok() {
                tokio::time::sleep(SETTLE).await;
            }
            ended(result);
        });
        runtime.spawn(async move {
            showing(
                rpc.call::<_, String>("get_backup_qr", (account_id,))
                    .await
                    .map_err(|err| err.to_string()),
            );
        });
    }

    /// Stop an offer that is running.
    pub fn cancel(&mut self) {
        if !self.running {
            return;
        }
        self.stop();
    }

    /// Core event: transfer progress only.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if !self.running || self.account_id == 0 || context_id != self.account_id {
            return;
        }
        if kind.to_string() != "ImexProgress" {
            return;
        }
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        let Some(permille) = json::u32_opt(&payload, "progress") else {
            return;
        };
        // 1000 = device has profile.
        if permille >= DONE {
            self.set_permille(DONE);
            self.end();
            self.taken();
            return;
        }
        // 0 = failure; core's words follow from provider, so bar not dropped.
        if permille > 0 {
            self.set_permille(permille);
        }
    }

    /// End offer on page: nothing running, code stale, late answers ignored.
    fn end(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.set_running(false);
        // Code goes with provider.
        self.set_code(QString::default());
    }

    /// End offer here, stop provider; page done at once (no bar, Cancel waiting).
    fn stop(&mut self) {
        self.end();
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            // Fire and forget: answer lands nowhere.
            rpc.call::<_, ()>("stop_ongoing_process", (account_id,))
                .await
                .ok();
        });
    }

    /// Set [`Self::running`] and announce it.
    fn set_running(&mut self, running: bool) {
        if self.running != running {
            self.running = running;
            self.running_changed();
        }
    }

    /// Set [`Self::code`] and announce it.
    fn set_code(&mut self, code: QString) {
        if self.code.to_string() != code.to_string() {
            self.code = code;
            self.code_changed();
        }
    }

    /// Set [`Self::permille`] and announce it.
    fn set_permille(&mut self, permille: u32) {
        if self.permille != permille {
            self.permille = permille;
            self.permille_changed();
        }
    }
}
