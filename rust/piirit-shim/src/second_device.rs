//! Offering one of this phone's profiles to a second device.
//!
//! The other half of taking a profile over from a device (`signup.rs`,
//! `RestoreProfilePage.qml`), and the half this app did not have: Piirit
//! could be *added* to a setup another Delta Chat was already holding,
//! but could not be the device holding it. Both ends are the core's own
//! backup transfer over the local network -- one device offers, the
//! other reads a code and takes a copy -- so what is here is the offer.
//!
//! Two core calls, and both are running at once.
//!
//! `provide_backup` starts the provider and does not answer until a
//! device has taken the profile or the provider is stopped, which is
//! also how this knows the transfer is over. `get_backup_qr` answers
//! with the text of the code the other device has to read; it blocks
//! until the provider is ready, so the order the two go out in does not
//! matter, and it gives up after a minute rather than deadlocking. The
//! transport correlates replies by id (`deltachat-jsonrpc`), so two
//! calls outstanding on one account is nothing new here.
//!
//! Like the export next door (`backup.rs`) this is a small object a page
//! owns rather than anything on the core object: the core's provider
//! takes an account id, so "which profile is offered" is the whole of
//! what it is about, and a phone with three profiles can offer any of
//! them.
//!
//! Two things worth knowing about what the core does under this.
//!
//! **The profile stops fetching while the offer is up.** The core pauses
//! IO for the account when the provider is prepared and resumes it when
//! the provider ends, however it ends. Nothing here starts it again --
//! doing so would race the core's own resume -- and the page says as
//! much, because a profile that has quietly stopped collecting mail is
//! worth a line rather than a bug report.
//!
//! **The progress events are the transfer's, not the offer's, and they
//! are the only thing that says it worked.** The core reports the
//! provider in `ImexProgress`, the same events an import or an export is
//! reported in, and none are emitted until a device actually connects: a
//! code sitting on screen with nobody looking at it reports nothing at
//! all. So the page is a code until the first event and a bar after it,
//! and the object only listens while it is the one that asked
//! (`handle_event`), as `backup.rs` does.
//!
//! What the call answers cannot stand in for those events. Pinned
//! against the bundled core: a provider a device took ends `Ok`, and so
//! does a provider `stop_ongoing_process` ended -- both are the provider
//! having finished, which is not the same as the profile having gone
//! anywhere. Only the last progress tells the two apart: 1000 for a
//! transfer that completed, 0 for one that did not. So 1000 is what
//! `taken` is emitted on, and it is emitted as it arrives rather than
//! held until the call answers; an offer that ends any other way without
//! the reader stopping it is `stalled`, which is this app's own finding
//! and not a message from the core. An offer the reader stops is over
//! the moment they say so: the page keeps nothing up waiting on a call
//! whose answer it will throw away, and having ended it here is what
//! makes that answer silent.
//!
//! Which of the two lands first is a race, and not one to decide the
//! verdict by. The events are polled on a call of their own
//! (`deltachat-jsonrpc`'s event loop), so the 1000 emitted before the
//! provider answers can arrive after it -- as it does against the
//! double, whose poll is a loop with a wait in it. So a provider that
//! answers `Ok` is not read as an answer at all until the event has had
//! [`SETTLE`] to turn up: by then a transfer that finished has said so
//! and this has nothing left to do, and one that did not is the stall.
//! The wait is only ever spent on an offer that is over and went
//! nowhere; a hand-over is announced the moment the core reports it,
//! and the core's own words for a provider it refused are passed on as
//! they arrive.

use std::time::Duration;

use qmetaobject::*;

use crate::core::connection;
use crate::json;

/// The permille the core counts an `ImexProgress` up to when whatever it
/// was reporting is done.
const DONE: u32 = 1000;

/// How long the progress that says how an offer ended is given to arrive
/// after the provider's own answer has. Long enough for an event behind
/// a poll on a loaded phone, short enough that an offer nobody came for
/// is reported while the reader is still looking at the page.
const SETTLE: Duration = Duration::from_secs(1);

/// One profile, offered to a device that reads the code.
///
/// ```qml
/// SecondDevice { id: device; account_id: page.accountId }
/// QrCode { text: device.code }
/// Button { onClicked: device.offer() }
/// ```
#[derive(QObject, Default)]
pub struct SecondDevice {
    base: qt_base_class!(trait QObject),

    /// Which profile is offered. The core's provider takes an account,
    /// so this is the whole of what "which profile" means. Set once, by
    /// the page that owns this object, from the profile it was opened
    /// for.
    pub account_id: qt_property!(u32),

    /// True from [`Self::offer`] until the provider has ended, whether
    /// a device took the profile or the offer was stopped.
    pub running: qt_property!(bool; NOTIFY running_changed),
    /// Emitted when [`Self::running`] changes.
    pub running_changed: qt_signal!(),

    /// The text of the code the other device has to read, empty until
    /// the core has one and again once the offer is over. Nothing here
    /// reads it: it is a `DCBACKUP` payload for the core at the other
    /// end -- the digit after it is the core's transfer version, and
    /// not this app's business -- drawn as a code and shown as text for
    /// a camera that will not read the picture.
    pub code: qt_property!(QString; NOTIFY code_changed),
    /// Emitted when [`Self::code`] changes.
    pub code_changed: qt_signal!(),

    /// How far the transfer has got, in permille, as the core counts it.
    /// Stays 0 for as long as the code is up with nobody reading it:
    /// the core reports the transfer, not the waiting.
    pub permille: qt_property!(u32; NOTIFY permille_changed),
    /// Emitted when [`Self::permille`] changes.
    pub permille_changed: qt_signal!(),

    /// Offer this profile. Answers on `taken`, `stalled` or `error`, and
    /// puts the code up on [`Self::code`] on the way.
    pub offer: qt_method!(fn(&mut self)),

    /// Stop an offer that is running. However the provider then answers
    /// -- `Ok`, as the bundled core does -- it is the reader's own
    /// doing, and neither an outcome nor a failure to report.
    pub cancel: qt_method!(fn(&mut self)),

    /// Feed a `core_event` in. Events for other accounts, and every kind
    /// but the progress of this transfer, are ignored -- so a page can
    /// connect this without filtering first.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// A device read the code and has the profile now.
    pub taken: qt_signal!(),

    /// The offer ended with nobody having taken the profile, and the
    /// reader did not stop it: the core had no words for it, so the
    /// page supplies them. A reason of this app's own, like the ones
    /// `signup.rs` hands the restore page.
    pub stalled: qt_signal!(),

    /// Nothing was offered, in the core's own words.
    pub error: qt_signal!(message: QString),

    /// Counts offers, and moves on once one is over as far as the page is
    /// concerned -- taken, stopped, or reported -- so whatever a provider
    /// answers after that, the code it shows or the way it ended, lands
    /// nowhere. Per offer rather than a flag the next offer clears: a
    /// stopped provider answers a second after the stop (`SETTLE`), by
    /// when the reader may have asked for the code again, and its answer
    /// then ended the new offer on the page with the new provider still
    /// running in the core, where nothing could stop it any more.
    generation: u64,
}

impl SecondDevice {
    /// Offer this profile to a device that reads the code.
    pub fn offer(&mut self) {
        // A second provider while one is running would be refused by the
        // core -- one ongoing process per account -- and the refusal
        // would land on the page as a failure of the offer it is already
        // showing.
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
            // The offer is already over here: the transfer was reported
            // done, or the reader stopped it -- and may have asked for
            // another since. Either way the provider answering afterwards
            // -- `Ok`, or its refusal of the stop -- is not news.
            if this.borrow().generation != generation {
                return;
            }
            this.borrow_mut().end();
            match result {
                // The provider finished without the transfer having
                // finished: nobody came, or whoever did went away. The
                // core has no words for that, so the page has them.
                Ok(()) => this.borrow().stalled(),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let showing = queued_callback(move |result: Result<String, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // The provider can be over before the code arrives -- it was
            // refused outright, or the reader gave up while the core was
            // still preparing -- and a code put up after that belongs to
            // nothing.
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok(code) => this.borrow_mut().set_code(code.into()),
                // No code is no offer: a page showing nothing to read is
                // a provider running for no one, so it is stopped here
                // and its own refusal is not reported on top of this.
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
            // An `Ok` says the provider finished and nothing more (see
            // the module doc), so the progress that says how is given a
            // moment to catch up before this is read as anything. A
            // refusal carries its own words and is passed on at once.
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

    /// Take in one core event: the progress of the transfer, or nothing.
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
        // 1000 is the whole of what says a device has the profile: the
        // provider's own call answers `Ok` either way (see the module
        // doc), so this is the outcome rather than a step towards it.
        if permille >= DONE {
            self.set_permille(DONE);
            self.end();
            self.taken();
            return;
        }
        // The core reports a failure as progress 0, and the provider's
        // own call answers a moment later -- with the core's words when
        // it has any. A bar that drops back to nothing first says less
        // than that and looks like a restart.
        if permille > 0 {
            self.set_permille(permille);
        }
    }

    /// End the offer on the page: nothing is running, the code is not
    /// worth reading any more, and whatever answers next has nothing
    /// left to report.
    fn end(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.set_running(false);
        // The code goes with the provider: what is left on screen after
        // it has ended is a code nothing is listening on any more.
        self.set_code(QString::default());
    }

    /// End the offer here and stop the provider in the core.
    ///
    /// The page is done with it the moment this is called rather than
    /// when the core gets round to answering: an offer that has been
    /// stopped has nothing left to show, and a page still holding a bar
    /// and a Cancel button over it would be waiting on a call whose
    /// answer it is going to throw away. Ending it first is also what
    /// makes that answer silent, whatever it turns out to be.
    fn stop(&mut self) {
        self.end();
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            // Fire and forget: the provider's own answer is what clears
            // the page, and it comes either way.
            let _ = rpc
                .call::<_, ()>("stop_ongoing_process", (account_id,))
                .await;
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
