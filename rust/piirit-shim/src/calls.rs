//! Calls: what the core says about one, and making one.
//!
//! A call is a message -- `Viewtype::Call` -- and the core keeps its whole
//! state: ringing, accepted, ended, and how long it lasted. It also does
//! all of the signalling, which is that message and two hidden ones after
//! it. What it does not do is the media: the SDP it carries is opaque to
//! it, and producing one is the job of a WebRTC stack. Here that is the
//! browser engine, running upstream's calls-webapp page, which the call
//! host serves (`call_host.rs`).
//!
//! Two things live here:
//!
//! - [`summary`], which a message row reads so that a call draws as one
//!   rather than as the sentence the core writes into its text, which is
//!   English whatever the phone's language is;
//! - [`Call`], the one call the app can be in: ringing, answered, placed,
//!   and ended, with the page it runs in.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::call_host::{self, Host, Report, Setup};
use crate::core::connection;
use crate::json;

/// What the core says about one call message (`call_info`).
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Summary {
    /// The core's name for where the call stands: `Alerting`, `Active`,
    /// `Completed`, `Missed`, `Declined` or `Canceled`.
    pub(crate) state: String,
    /// The call was started as a video call.
    pub(crate) has_video: bool,
    /// Seconds, for a completed call; 0 for any other.
    pub(crate) duration: i32,
    /// The caller's SDP offer, which is what answering a call starts
    /// from. Opaque here: the core carries it and never reads it, and so
    /// does this.
    pub(crate) sdp_offer: String,
}

impl Summary {
    /// Read the core's `CallInfo`. Its state is an object tagged by
    /// `kind`, and only `Completed` carries anything beside the tag.
    pub(crate) fn from_json(info: &serde_json::Value) -> Self {
        Self {
            state: json::str_at(info, "/state/kind").to_string(),
            has_video: json::flag(info, "hasVideo"),
            duration: json::i32_at(info, "/state/duration"),
            sdp_offer: json::str_at(info, "sdpOffer").to_string(),
        }
    }
}

/// What the core says about the call `message_id` is. An error for a
/// message that is not a call, in the core's own words.
pub(crate) async fn summary(
    rpc: &RpcClient,
    account_id: u32,
    message_id: u32,
) -> Result<Summary, String> {
    let answer: serde_json::Value = rpc
        .call("call_info", (account_id, message_id))
        .await
        .map_err(|err| err.to_string())?;
    Ok(Summary::from_json(&answer))
}

/// The page's own option a call's page is loaded with. Audio only, for
/// now: the page asks for no camera, sends no video and drops what video
/// comes in, so a call answered here is a voice call whatever the other
/// end started it as. Video is the half of this that is unproven on a
/// phone (docs/CALLS.md).
const PAGE_OPTIONS: &str = "disableVideoCompletely";

/// What one call's page needs before the host can come up.
struct Prepared {
    ice_servers: String,
    avatar_path: String,
}

/// The one call the app can be in.
///
/// ```qml
/// Call { id: call }
/// // call.place(accountId, chatId, false)   -> state "starting", "calling", ...
/// // onRinging: ...call.answer() or call.hang_up()
/// WebView { url: call.url }
/// ```
///
/// One object for the whole window, because a call belongs to no page:
/// one can come in whatever is on screen, and a page the reader leaves is
/// not a call the reader has hung up. It moves through `state`:
///
/// - `""`: no call.
/// - `ringing`: a call came in, and nobody here has answered it yet.
/// - `starting`: the page is coming up, and asking for the microphone.
/// - `calling`: the core has placed the call, and the other end is
///   ringing.
/// - `connecting`: both ends have agreed to talk, and the media has not
///   come through yet.
/// - `connected`: the two ends can hear each other.
/// - `reconnecting`: the connection dropped, and may come back.
/// - `ended`: over, for the reason in `end_reason`, until `reset`.
#[derive(QObject, Default)]
pub struct Call {
    base: qt_base_class!(trait QObject),

    /// The account the call belongs to. 0 without a call.
    pub account_id: qt_property!(u32; NOTIFY call_changed),
    /// The chat the call is in.
    pub chat_id: qt_property!(u32; NOTIFY call_changed),
    /// The call's message: what the core knows the call as. 0 for an
    /// outgoing call the core has not placed yet.
    pub message_id: qt_property!(u32; NOTIFY call_changed),
    /// The call came in, rather than being made here.
    pub incoming: qt_property!(bool; NOTIFY call_changed),
    /// The call was started as a video call, at whichever end.
    pub has_video: qt_property!(bool; NOTIFY call_changed),
    /// Who is at the other end, as the chat is named.
    pub peer_name: qt_property!(QString; NOTIFY call_changed),
    /// Their picture, a file the core keeps; empty for none.
    pub peer_avatar: qt_property!(QString; NOTIFY call_changed),
    /// The core's colour for them, `#rrggbb`.
    pub peer_color: qt_property!(QString; NOTIFY call_changed),
    /// Emitted when any of the above changes.
    pub call_changed: qt_signal!(),

    /// Where the call is; see the list above.
    pub state: qt_property!(QString; NOTIFY state_changed),
    /// Why an ended call ended: `hung-up` from here, `declined` here
    /// while it rang, `remote` by the other end, `answered-elsewhere` on
    /// another of this account's devices, `missed` after ringing here
    /// unanswered, `failed` when the connection or the core gave up.
    pub end_reason: qt_property!(QString; NOTIFY state_changed),
    /// When the two ends first heard each other, in Unix seconds; 0
    /// until then. What a running clock on the call screen counts from.
    pub connected_at: qt_property!(f64; NOTIFY state_changed),
    /// Emitted when the state, its reason or its clock changes.
    pub state_changed: qt_signal!(),

    /// The microphone is off: the other end hears nothing from here.
    /// On again for every new call.
    pub muted: qt_property!(bool; NOTIFY muted_changed),
    /// Emitted when the microphone goes off or on.
    pub muted_changed: qt_signal!(),

    /// Where the call's page is, with the command it opens on. Empty
    /// until the host is up, and again once the call is over. What a
    /// `WebView` is pointed at.
    pub url: qt_property!(QString; NOTIFY url_changed),
    /// Emitted when the page comes or goes.
    pub url_changed: qt_signal!(),

    /// A call came in and is ringing here.
    pub ringing: qt_signal!(),
    /// The call is over; `end_reason` says why.
    pub ended: qt_signal!(reason: QString),
    /// Something failed. The message is the core's own.
    pub error: qt_signal!(message: QString),

    /// Place a call into a chat. False, and nothing done, when the app is
    /// already in one.
    pub place: qt_method!(fn(&mut self, account_id: u32, chat_id: u32, video: bool) -> bool),
    /// Take up a call that is ringing from its row in the chat: one that
    /// rang while nothing here was listening -- the app was starting, or
    /// the event came and went -- but that the core still has ringing.
    /// Answers on `state`, which becomes `ringing` without the `ringing`
    /// signal: the reader is already looking at it, and nothing need ring.
    pub pick_up: qt_method!(fn(&mut self, account_id: u32, chat_id: u32, message_id: u32)),
    /// Answer the call that is ringing.
    pub answer: qt_method!(fn(&mut self)),
    /// Decline the call that is ringing, or end the one in progress.
    pub hang_up: qt_method!(fn(&mut self)),
    /// Forget an ended call, so the next one can start.
    pub reset: qt_method!(fn(&mut self)),
    /// Turn the microphone off, or back on, for the call under way.
    pub mute: qt_method!(fn(&mut self, on: bool)),
    /// Apply one core event. Only the four call events are acted on.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// How long a call may ring here before it is taken to have rung
    /// out, in milliseconds; 0 for the app's own limit, a little over the
    /// core's 120 seconds. Only a test sets it.
    pub ring_limit_ms: qt_property!(u32),

    /// The loopback host the page is served from, while there is a page.
    host: Option<Host>,
    /// The caller's offer, for a call that came in: what answering it
    /// starts from.
    offer: String,
    /// Counts calls, so an answer meant for an earlier one -- a host that
    /// came up after it ended, the other end's picture -- lands nowhere.
    generation: u64,
    /// Call events for this account that arrived while an outgoing call
    /// had no message yet: the core's event can overtake the host's
    /// report of the id it placed the call as, and an answer dropped for
    /// not naming a known call is a call that never connects. Read again
    /// once the id is known; see `take_report`.
    early: Vec<(String, String)>,
    /// Set once the core needs telling nothing more about this call: it
    /// ended the call itself, or another of this account's devices took
    /// it. A host let go after that -- the one up now, or one still
    /// coming up -- goes without `end_call`, which for a call answered
    /// elsewhere would end it on the device that answered. One per call,
    /// so a host coming up late reads the flag of the call it was for.
    settled: Arc<AtomicBool>,
}

/// How long a call may ring here before it is taken to have rung out
/// without the core saying so. The core rings a call for 120 seconds from
/// when it was sent, and says `CallEnded` when that is up -- once, from a
/// timer that lives only in the server that heard the call. An event
/// channel that overflowed, or a server that died and was started again,
/// loses it, and without this the phone would ring until somebody
/// declined. A little over the core's own, so that when the core does
/// say it, its word is the one taken.
const RING_LIMIT: std::time::Duration = std::time::Duration::from_secs(125);

/// The most early events kept for one call. They are for a moment that
/// lasts as long as one round trip to the core, and anything past this
/// is not about the call being placed.
const EARLY_EVENTS: usize = 16;

impl Call {
    /// Whether a call is under way, in any state short of over.
    fn busy(&self) -> bool {
        !matches!(self.state.to_string().as_str(), "" | "ended")
    }

    fn set_state(&mut self, state: &str) {
        if self.state.to_string() != state {
            self.state = state.into();
            self.state_changed();
        }
    }

    /// A new call, whichever way it came: everything about the last one
    /// forgotten, and answers meant for it turned away.
    fn begin(&mut self, account_id: u32, chat_id: u32, message_id: u32, incoming: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.host = None;
        self.early.clear();
        self.settled = Arc::new(AtomicBool::new(false));
        self.account_id = account_id;
        self.chat_id = chat_id;
        self.message_id = message_id;
        self.incoming = incoming;
        self.peer_name = QString::default();
        self.peer_avatar = QString::default();
        self.peer_color = QString::default();
        self.end_reason = QString::default();
        self.connected_at = 0.0;
        self.call_changed();
        if self.muted {
            self.muted = false;
            self.muted_changed();
        }
        self.load_peer();
    }

    /// Place a call.
    pub fn place(&mut self, account_id: u32, chat_id: u32, video: bool) -> bool {
        if self.busy() || account_id == 0 || chat_id == 0 {
            return false;
        }
        self.begin(account_id, chat_id, 0, false);
        self.has_video = video;
        self.offer.clear();
        self.call_changed();
        self.set_state("starting");
        self.serve("startCall".to_string());
        true
    }

    /// Take up a ringing call from its row.
    pub fn pick_up(&mut self, account_id: u32, chat_id: u32, message_id: u32) {
        if self.busy() || account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        // Asked of the core rather than taken from the row: the offer is
        // not on a row, and the call may have stopped ringing since the
        // row was drawn.
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |found: Result<Summary, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            let Ok(call) = found else { return };
            if call.state != "Alerting" || this.borrow().busy() {
                return;
            }
            let mut this_mut = this.borrow_mut();
            this_mut.begin(account_id, chat_id, message_id, true);
            this_mut.has_video = call.has_video;
            this_mut.offer = call.sdp_offer;
            this_mut.call_changed();
            this_mut.set_state("ringing");
            this_mut.watch_ringing();
        });
        runtime.spawn(async move {
            done(summary(&rpc, account_id, message_id).await);
        });
    }

    /// Answer the ringing call.
    pub fn answer(&mut self) {
        if self.state.to_string() != "ringing" {
            return;
        }
        self.set_state("starting");
        let command = call_host::command("acceptCall", &self.offer);
        self.serve(command);
    }

    /// Decline, or hang up.
    pub fn hang_up(&mut self) {
        match self.state.to_string().as_str() {
            "" | "ended" => {}
            // Declined here: the core tells the caller, and this
            // account's other devices, and says `CallEnded` -- which,
            // arriving at an ended call, is not acted on twice.
            "ringing" => {
                self.end_call();
                self.finish("declined");
            }
            // Dropping the host is what tells the core; see its `Drop`.
            _ => self.finish("hung-up"),
        }
    }

    /// The microphone, off or on. The page has a switch of its own for
    /// it, and is told to press it: it is the page that keeps whether the
    /// microphone is on, and tells the other end. A page not up yet is
    /// told once it is; see `serve`.
    pub fn mute(&mut self, on: bool) {
        if !self.busy() || self.muted == on {
            return;
        }
        self.muted = on;
        self.muted_changed();
        if let Some(host) = &self.host {
            host.command(call_host::mute_command(on));
        }
    }

    /// Forget an ended call.
    pub fn reset(&mut self) {
        if self.state.to_string() == "ended" {
            self.end_reason = QString::default();
            self.set_state("");
        }
    }

    /// The call is over: the page goes, and the reason is said.
    fn finish(&mut self, reason: &str) {
        if self.close() {
            self.say_ended(reason);
        }
    }

    /// Everything about ending but the reason: the page goes, and answers
    /// still on their way are turned away. False when there was no call
    /// to end.
    fn close(&mut self) -> bool {
        if !self.busy() {
            return false;
        }
        self.generation = self.generation.wrapping_add(1);
        self.host = None;
        if !self.url.to_string().is_empty() {
            self.url = QString::default();
            self.url_changed();
        }
        self.end_reason = QString::default();
        self.set_state("ended");
        true
    }

    /// Say why the call ended.
    fn say_ended(&mut self, reason: &str) {
        self.end_reason = reason.into();
        self.state_changed();
        self.ended(reason.into());
    }

    /// The core needs telling nothing more about this call; see
    /// `settled`.
    fn settle(&self) {
        self.settled.store(true, Ordering::SeqCst);
        if let Some(host) = &self.host {
            host.settle();
        }
    }

    /// Tell the core to end the call, without waiting for it.
    fn end_call(&self) {
        let (account_id, message_id) = (self.account_id, self.message_id);
        if message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            let _: Result<serde_json::Value, _> =
                rpc.call("end_call", (account_id, message_id)).await;
        });
    }

    /// Read who is at the other end: the chat's name and picture, which
    /// in a one-to-one chat are theirs.
    fn load_peer(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let generation = self.generation;
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |peer: Option<(String, String, String)>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            let Some((name, avatar, color)) = peer else {
                return;
            };
            {
                let mut this_mut = this.borrow_mut();
                this_mut.peer_name = name.into();
                this_mut.peer_avatar = avatar.into();
                this_mut.peer_color = color.into();
            }
            this.borrow().call_changed();
        });
        runtime.spawn(async move {
            let info: Option<serde_json::Value> = rpc
                .call("get_basic_chat_info", (account_id, chat_id))
                .await
                .ok();
            done(info.map(|chat| {
                (
                    json::str_at(&chat, "name").to_string(),
                    json::str_at(&chat, "profileImage").to_string(),
                    json::str_at(&chat, "color").to_string(),
                )
            }));
        });
    }

    /// Bring the page up, opening on `command`.
    fn serve(&mut self, command: String) {
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            self.finish("failed");
            return;
        };
        let setup = (
            self.account_id,
            self.chat_id,
            self.message_id,
            self.has_video,
        );
        let generation = self.generation;

        // Reports from the host, raised on the Qt thread.
        let report_ptr: QPointer<Self> = QPointer::from(&*self);
        let raise = queued_callback(move |report: Report| {
            let Some(this) = report_ptr.as_pinned() else {
                return;
            };
            if this.borrow().generation != generation {
                return;
            }
            this.borrow_mut().take_report(report);
        });
        let report: call_host::Reporter = Arc::new(raise);

        let settled = Arc::clone(&self.settled);
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Host, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // A call hung up while its page was coming up: dropping the
            // host here is what lets it go -- quietly, for a call the
            // core has nothing more to hear about.
            if this.borrow().generation != generation {
                if let (Ok(host), true) = (&result, settled.load(Ordering::SeqCst)) {
                    host.settle();
                }
                return;
            }
            match result {
                Ok(host) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.url = host.url(PAGE_OPTIONS, &command).into();
                        // Muted before there was a page to tell.
                        if this_mut.muted {
                            host.command(call_host::mute_command(true));
                        }
                        this_mut.host = Some(host);
                    }
                    this.borrow().url_changed();
                }
                Err(err) => {
                    this.borrow().error(err.into());
                    this.borrow_mut().finish("failed");
                }
            }
        });

        runtime.spawn(async move {
            let (account_id, chat_id, message_id, has_video) = setup;
            let prepared = prepare(&rpc, account_id, chat_id).await;
            let host = call_host::start(
                Arc::clone(&rpc),
                Setup {
                    account_id,
                    chat_id,
                    message_id,
                    has_video,
                    ice_servers: prepared.ice_servers,
                    avatar_path: prepared.avatar_path,
                },
                report,
            )
            .await;
            done(host);
        });
    }

    /// What the host says, applied.
    fn take_report(&mut self, report: Report) {
        match report {
            Report::Placed(message_id) => {
                self.message_id = message_id;
                self.call_changed();
                if self.state.to_string() == "starting" {
                    self.set_state("calling");
                }
                let account_id = self.account_id;
                for (kind, payload) in std::mem::take(&mut self.early) {
                    self.handle_event(account_id, kind.into(), payload.into());
                }
            }
            Report::Accepted => {
                if self.state.to_string() == "starting" {
                    self.set_state("connecting");
                }
            }
            Report::Connection(ice) => self.take_connection(&ice),
            // The core has already been told, by the host.
            Report::EndedByPage => self.finish("hung-up"),
            Report::Failed(message) => {
                self.error(message.into());
                self.finish("failed");
            }
        }
    }

    /// The peer connection's own word for how it is doing.
    fn take_connection(&mut self, ice: &str) {
        if !self.busy() || self.state.to_string() == "ringing" {
            return;
        }
        match ice {
            "connected" | "completed" => {
                if self.connected_at == 0.0 {
                    self.connected_at = now();
                    self.state_changed();
                }
                self.set_state("connected");
            }
            // ICE may find its way back by itself; "failed" is when it
            // has stopped trying.
            "disconnected" if self.state.to_string() == "connected" => {
                self.set_state("reconnecting");
            }
            "failed" => self.finish("failed"),
            _ => {}
        }
    }

    /// Apply one core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        let kind = kind.to_string();
        if !matches!(
            kind.as_str(),
            "IncomingCall" | "IncomingCallAccepted" | "OutgoingCallAccepted" | "CallEnded"
        ) {
            return;
        }
        // About a call this end is placing, before the core has said what
        // it placed it as: kept for when it has.
        if kind != "IncomingCall"
            && context_id == self.account_id
            && !self.incoming
            && self.message_id == 0
            && self.busy()
        {
            if self.early.len() < EARLY_EVENTS {
                self.early.push((kind, payload_json.to_string()));
            }
            return;
        }
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        // The call events are the one part of the core's stream written
        // in snake_case: `msg_id`, not `msgId` (docs/CALLS.md).
        let Some(message_id) = json::u32_opt(&payload, "msg_id") else {
            return;
        };
        let ours = context_id == self.account_id && message_id == self.message_id;
        match kind.as_str() {
            "IncomingCall" => {
                // One call at a time, as both reference clients have it:
                // a second one rings nowhere, and is a missed call once
                // the core gives up on it.
                if self.busy() {
                    return;
                }
                let chat_id = json::u32_at(&payload, "chat_id");
                self.begin(context_id, chat_id, message_id, true);
                self.has_video = json::flag(&payload, "has_video");
                self.offer = json::str_at(&payload, "place_call_info").to_string();
                self.call_changed();
                self.set_state("ringing");
                self.watch_ringing();
                self.ringing();
            }
            // Answered: here, which the host has reported already, or on
            // another device. The core says the latter only of a call
            // this device has not accepted itself, and turns this
            // device's own accept into nothing once it has -- so from any
            // state short of over, answered here or not, the call is that
            // device's now, and letting it go must not end it there.
            "IncomingCallAccepted" if ours => {
                if !json::flag(&payload, "from_this_device") && self.busy() {
                    self.settle();
                    self.finish("answered-elsewhere");
                }
            }
            // The other end answered: its answer goes to the page. Every
            // one of this account's devices hears it, and only the one
            // that placed the call has a page to give it to.
            "OutgoingCallAccepted" if ours && !self.incoming => {
                if let Some(host) = &self.host {
                    let answer = json::str_at(&payload, "accept_call_info");
                    host.command(call_host::command("onAnswer", answer));
                }
                if self.state.to_string() == "calling" {
                    self.set_state("connecting");
                }
            }
            "CallEnded" if ours && self.busy() => {
                // Nothing to tell the core: it is the one saying so.
                self.settle();
                if self.state.to_string() == "ringing" {
                    self.finish_unanswered();
                } else {
                    self.finish("remote");
                }
            }
            _ => {}
        }
    }

    /// Stop a call ringing that has rung past [`RING_LIMIT`] with nothing
    /// from the core. By then the core counts it as over whatever became
    /// of its own timer -- a ringing call goes stale 120 seconds after it
    /// was sent, and `call_info` says so -- so it is ended as one that
    /// rang out, and the core is asked, as ever, which way.
    fn watch_ringing(&self) {
        let Some((_, runtime)) = connection() else {
            return;
        };
        let limit = match self.ring_limit_ms {
            0 => RING_LIMIT,
            ms => std::time::Duration::from_millis(u64::from(ms)),
        };
        let generation = self.generation;
        let ptr: QPointer<Self> = QPointer::from(self);
        let done = queued_callback(move |()| {
            let Some(this) = ptr.as_pinned() else { return };
            // Answered, declined or ended meanwhile: answering keeps the
            // call, ending it starts another count.
            if this.borrow().generation != generation
                || this.borrow().state.to_string() != "ringing"
            {
                return;
            }
            this.borrow_mut().finish_unanswered();
        });
        runtime.spawn(async move {
            tokio::time::sleep(limit).await;
            done(());
        });
    }

    /// A call that rang here and ended unanswered: missed, or declined on
    /// another of this account's devices. The core knows which, and the
    /// difference is whether it is worth a notification -- so the ringing
    /// stops at once, and only the reason waits for the core's answer.
    fn finish_unanswered(&mut self) {
        let (account_id, message_id) = (self.account_id, self.message_id);
        if !self.close() {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.say_ended("missed");
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let generation = self.generation;
        let done = queued_callback(move |state: String| {
            let Some(this) = ptr.as_pinned() else { return };
            // Another call has begun since, and this one's reason is
            // nobody's business any more.
            if this.borrow().generation != generation {
                return;
            }
            let reason = if state == "Declined" {
                "declined"
            } else {
                "missed"
            };
            this.borrow_mut().say_ended(reason);
        });
        runtime.spawn(async move {
            let state = summary(&rpc, account_id, message_id)
                .await
                .map(|call| call.state)
                .unwrap_or_default();
            done(state);
        });
    }
}

/// What the host needs that only the core knows: the ICE servers, and
/// the other end's picture. Neither is worth failing a call over, so a
/// question the core will not answer is answered with nothing.
async fn prepare(rpc: &RpcClient, account_id: u32, chat_id: u32) -> Prepared {
    let ice_servers: String = rpc
        .call("ice_servers", (account_id,))
        .await
        .unwrap_or_else(|_| "[]".to_string());
    let chat: Option<serde_json::Value> = rpc
        .call("get_basic_chat_info", (account_id, chat_id))
        .await
        .ok();
    Prepared {
        ice_servers,
        avatar_path: chat
            .map(|chat| json::str_at(&chat, "profileImage").to_string())
            .unwrap_or_default(),
    }
}

/// Now, in Unix seconds.
fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::Summary;
    use serde_json::json;

    #[test]
    fn a_call_reads_as_the_core_serialises_it() {
        // Shapes taken from deltachat-rpc-server 2.62 itself: camelCase,
        // the state tagged by `kind`.
        let ringing = Summary::from_json(&json!({
            "hasVideo": true,
            "sdpOffer": "v=0",
            "state": {"kind": "Alerting"},
        }));
        assert_eq!(ringing.state, "Alerting");
        assert!(ringing.has_video);
        assert_eq!(ringing.duration, 0);
        assert_eq!(ringing.sdp_offer, "v=0");

        let done = Summary::from_json(&json!({
            "hasVideo": false,
            "sdpOffer": "v=0",
            "state": {"kind": "Completed", "duration": 185},
        }));
        assert_eq!(done.state, "Completed");
        assert!(!done.has_video);
        assert_eq!(done.duration, 185);
    }

    #[test]
    fn a_shape_this_does_not_know_reads_as_nothing_at_all() {
        let odd = Summary::from_json(&json!({"state": "Completed"}));
        assert_eq!(odd, Summary::default());
    }
}
