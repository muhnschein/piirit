//! Offer one profile to a second device.
//!
//! Other half of profile takeover (`signup.rs`): core's backup transfer over LAN.
//! This device offers; other reads the code.
//!
//! `provide_backup` runs until device takes profile or provider stopped. `get_backup_qr`
//! gives code text, blocks until provider ready, gives up after a minute. Two calls on
//! one account is fine: transport correlates by id.
//!
//! Owned by a page, like `backup.rs`: provider takes account id.
//!
//! Core pauses IO for account while offer is up, resumes when provider ends. Nothing
//! here restarts it (races core); page says so.
//!
//! `ImexProgress` (transfer's, not offer's) is the only proof of success: none until device
//! connects, so page shows code until first event, bar after; listens only while it asked
//! (`handle_event`), as `backup.rs`. Last progress: 1000 taken, 0 failed. Provider `Ok` alone
//! is not enough: bundled core answers `Ok` after stop too.
//! So `taken` fires on 1000 as it arrives. Offer ending otherwise, unstopped by reader,
//! is `stalled` (app's own word). Reader stop ends offer at once; late answer ignored.
//!
//! Race: progress is polled on its own call (event loop), may land after `Ok`. So `Ok` waits
//! [`SETTLE`] before read: finished transfer has reported by then; else stall. Wait only spent
//! on an offer that went nowhere. Hand-over announced at once; core refusals passed as they come.

use std::time::Duration;

use qmetaobject::*;

use crate::core::connection;
use crate::json;

/// Permille counted up to when done.
const DONE: u32 = 1000;

/// Grace for progress after provider `Ok`: long enough for an event behind a poll on a loaded
/// phone; short enough to report unclaimed offer while reader still on page.
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

    /// Code text for other device; empty outside an offer. `DCBACKUP` payload for other core
    /// (digit after is core's transfer version); nothing here reads it. Shown as text too, for
    /// cameras that will not read the QR.
    pub code: qt_property!(QString; NOTIFY code_changed),
    /// Emitted when [`Self::code`] changes.
    pub code_changed: qt_signal!(),

    /// Transfer progress, permille. 0 while code waits unread.
    pub permille: qt_property!(u32; NOTIFY permille_changed),
    /// Emitted when [`Self::permille`] changes.
    pub permille_changed: qt_signal!(),

    /// Offer profile. Answers `taken`, `stalled` or `error`; code goes on [`Self::code`].
    pub offer: qt_method!(fn(&mut self)),

    /// Stop running offer. Provider's answer after is the reader's doing: no outcome.
    pub cancel: qt_method!(fn(&mut self)),

    /// Feed `core_event` in. Other accounts and kinds ignored, so page needs no filter.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// A device read the code and has the profile now.
    pub taken: qt_signal!(),

    /// Ended with nobody taking profile, reader not stopping. App's own reason.
    pub stalled: qt_signal!(),

    /// Nothing offered, core's words.
    pub error: qt_signal!(message: QString),

    /// Offer counter. Once an offer is over for the page, late answers from its provider
    /// land nowhere. Per offer, not a flag: stopped provider answers `SETTLE` later, maybe after
    /// a new offer; it once ended the new offer on page with its provider running, unstoppable.
    generation: u64,
}

impl SecondDevice {
    /// Offer this profile to a device that reads the code.
    pub fn offer(&mut self) {
        // Core refuses a second provider per account; refusal would show as this offer's failure.
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
            // Offer already over (done, or stopped). Late provider answer is not news.
            if this.borrow().generation != generation {
                return;
            }
            this.borrow_mut().end();
            match result {
                // Provider finished without transfer: nobody came, or they left.
                Ok(()) => this.borrow().stalled(),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let showing = queued_callback(move |result: Result<String, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Provider can be over before code arrives (refused, or stopped while preparing).
            // Late code belongs to nothing.
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok(code) => this.borrow_mut().set_code(code.into()),
                // No code, no offer: stop provider; don't report its refusal on top.
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
            // `Ok` = provider finished (see module doc); give progress a moment.
            // Refusal passed at once.
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
        // 1000 = device has profile. Provider's `Ok` says nothing either way.
        if permille >= DONE {
            self.set_permille(DONE);
            self.end();
            self.taken();
            return;
        }
        // Failure is progress 0; provider's call answers after, with core's words.
        // Do not show a drop to 0 first.
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

    /// End offer here, stop provider. Page is done at once: waiting on an answer it
    /// will discard would hold bar and Cancel.
    fn stop(&mut self) {
        self.end();
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            // Fire and forget: page already ended above; provider's answer lands nowhere.
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
