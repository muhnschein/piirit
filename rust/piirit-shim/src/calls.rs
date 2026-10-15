//! Calls: what core says about one, and making one.
//!
//! Call is a message (`Viewtype::Call`). Core keeps whole state (ringing, accepted, ended,
//! duration) and all signalling (that message plus two hidden ones). Not media: SDP opaque
//! to core; made by `WebRTC` stack, here browser engine running upstream calls-webapp,
//! served by `call_host.rs`.
//!
//! - [`summary`]: message row reads it so call draws as call, not as core's text (English
//!   whatever phone language).
//! - [`Call`]: the one call app can be in (ringing, answered, placed, ended), with its page.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::call_host::{self, Host, Report, Setup};
use crate::core::connection;
use crate::json;

/// Core's view of call message (`call_info`).
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Summary {
    /// State: `Alerting`, `Active`, `Completed`, `Missed`, `Declined`, `Canceled`.
    pub(crate) state: String,
    /// Started as video call.
    pub(crate) has_video: bool,
    /// Seconds if completed; 0 otherwise.
    pub(crate) duration: i32,
    /// Caller's SDP offer; answering starts from it. Opaque: core and this carry, never read.
    pub(crate) sdp_offer: String,
}

impl Summary {
    /// Read `CallInfo`; state is object tagged `kind`, only `Completed` carries more.
    pub(crate) fn from_json(info: &serde_json::Value) -> Self {
        Self {
            state: json::str_at(info, "/state/kind").to_string(),
            has_video: json::flag(info, "hasVideo"),
            duration: json::i32_at(info, "/state/duration"),
            sdp_offer: json::str_at(info, "sdpOffer").to_string(),
        }
    }
}

/// Core's view of call `message_id`. Error if not call.
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

/// Page load option: whether camera starts on. Never `disableVideoCompletely`: drops other
/// end's picture too. Voice call = camera starts off; `call_video.js` keeps it closed until on.
fn page_options(camera: bool) -> &'static str {
    if camera {
        ""
    } else {
        "noOutgoingVideoInitially"
    }
}

/// Main back camera's index among back cameras: label nearest "1.0" in platform camera's
/// ring labels, in order ("1.0,0.5"); else first. Engine names back cameras alike, and
/// first listed may be wide one.
fn main_back_camera(labels: &str) -> usize {
    labels
        .split(',')
        .enumerate()
        .filter_map(|(place, label)| {
            let zoom: f64 = label
                .trim()
                .trim_end_matches(['x', '\u{d7}'])
                .parse()
                .ok()?;
            Some((place, (zoom - 1.0).abs()))
        })
        .fold(
            None,
            |best: Option<(usize, f64)>, (place, off)| match best {
                Some((_, nearest)) if nearest <= off => best,
                _ => Some((place, off)),
            },
        )
        .map_or(0, |(place, _)| place)
}

/// Audio policy id: 22nd field of `/proc/<pid>/stat` (start time, clock ticks since boot),
/// lower-case hex. Sailfish libpulse puts it on every stream (`policy.application.id`),
/// libresource sends same for resource set's streams; both split at single spaces, as does
/// this, so all three agree on any line. None if line short or field not a number.
fn audio_app_id(stat: &str) -> Option<String> {
    let ticks: u64 = stat.split(' ').nth(21)?.parse().ok()?;
    Some(format!("{ticks:x}"))
}

/// What page needs before host comes up.
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
/// One object for window: belongs to no page. Leaving page is not hang-up.
/// Moves through `state`:
///
/// - `""`: no call.
/// - `ringing`: came in, not answered.
/// - `starting`: page coming up, asking for mic.
/// - `calling`: placed, other end ringing.
/// - `connecting`: both agreed, media not through.
/// - `connected`: both hear each other.
/// - `reconnecting`: dropped, may return.
/// - `ended`: over, reason in `end_reason`, until `reset`.
// One flag per Qt property: QML binds each.
#[allow(clippy::struct_excessive_bools)]
#[derive(QObject, Default)]
pub struct Call {
    base: qt_base_class!(trait QObject),

    /// Account. 0 without call.
    pub account_id: qt_property!(u32; NOTIFY call_changed),
    /// Chat of call.
    pub chat_id: qt_property!(u32; NOTIFY call_changed),
    /// Call's message. 0 until placed.
    pub message_id: qt_property!(u32; NOTIFY call_changed),
    /// Came in, not made here.
    pub incoming: qt_property!(bool; NOTIFY call_changed),
    /// Video call, either end.
    pub has_video: qt_property!(bool; NOTIFY call_changed),
    /// Other end's name.
    pub peer_name: qt_property!(QString; NOTIFY call_changed),
    /// Their picture file, or empty.
    pub peer_avatar: qt_property!(QString; NOTIFY call_changed),
    /// Their colour, `#rrggbb`.
    pub peer_color: qt_property!(QString; NOTIFY call_changed),
    /// Emitted on any of above.
    pub call_changed: qt_signal!(),

    /// State; see list above.
    pub state: qt_property!(QString; NOTIFY state_changed),
    /// Why ended: `hung-up` (here), `declined` (here, ringing), `remote` (other end),
    /// `answered-elsewhere` (other device), `missed` (unanswered), `failed` (gave up).
    pub end_reason: qt_property!(QString; NOTIFY state_changed),
    /// Unix secs both ends first heard each other; 0 until then. Clock runs from it.
    pub connected_at: qt_property!(f64; NOTIFY state_changed),
    /// Emitted on state, reason or clock change.
    pub state_changed: qt_signal!(),

    /// Mic off: other end hears nothing. Reset to on for each new call.
    pub muted: qt_property!(bool; NOTIFY muted_changed),
    /// Emitted on mic change.
    pub muted_changed: qt_signal!(),

    /// Reader wants camera on: from start of video call, and when switched on. Kept in
    /// background, where camera itself is let go (`set_background`). Self-clears when
    /// camera will not open.
    pub camera: qt_property!(bool; NOTIFY video_changed),
    /// Back camera. Front again each call.
    pub rear_camera: qt_property!(bool; NOTIFY video_changed),
    /// Camera open and sending, per page.
    pub local_video: qt_property!(bool; NOTIFY video_changed),
    /// Other end's picture drawn, per page: sent, not switched off.
    pub remote_video: qt_property!(bool; NOTIFY video_changed),
    /// Camera painted a frame since last given one, per page. Black until then.
    pub local_painted: qt_property!(bool; NOTIFY video_changed),
    /// Same, for other end's picture.
    pub remote_painted: qt_property!(bool; NOTIFY video_changed),
    /// Emitted on any of six above.
    pub video_changed: qt_signal!(),

    /// Page URL with opening command; what `WebView` points at. Empty until host up and after call.
    pub url: qt_property!(QString; NOTIFY url_changed),
    /// Emitted when page comes or goes.
    pub url_changed: qt_signal!(),

    /// Ringing here.
    pub ringing: qt_signal!(),
    /// Call over; `end_reason` says why.
    pub ended: qt_signal!(reason: QString),
    /// Failed. Message is core's own.
    pub error: qt_signal!(message: QString),

    /// Place call. False if already in one.
    pub place: qt_method!(fn(&mut self, account_id: u32, chat_id: u32, video: bool) -> bool),
    /// Take up call core still has ringing, from its chat row: rang while nothing here
    /// listened (app starting, event missed). Answers on `state` -> `ringing`, without
    /// `ringing` signal: reader already looking at it.
    pub pick_up: qt_method!(fn(&mut self, account_id: u32, chat_id: u32, message_id: u32)),
    /// Answer ringing call.
    pub answer: qt_method!(fn(&mut self)),
    /// Decline ringing, or end call.
    pub hang_up: qt_method!(fn(&mut self)),
    /// Forget ended call.
    pub reset: qt_method!(fn(&mut self)),
    /// Mic off or on, call under way.
    pub mute: qt_method!(fn(&mut self, on: bool)),
    /// Camera on or off, call under way or ringing.
    pub set_camera: qt_method!(fn(&mut self, on: bool)),
    /// Flip camera facing.
    pub flip_camera: qt_method!(fn(&mut self)),
    /// Let page have mic, or shut it and keep it shut while call's sound moves
    /// (`CallAudio.qml`); must not move with mic open. Not mute: page's switch and what
    /// other end is told stay.
    pub set_microphone: qt_method!(fn(&mut self, on: bool)),
    /// App away or back: camera let go while away (other end told it is off), pictures not
    /// drawn. Back in front, camera reopens if on.
    pub set_background: qt_method!(fn(&mut self, away: bool)),
    /// Audio policy id for app sound: call's resource set names streams with it
    /// (`CallAudio.qml`). Empty if `/proc` lacks it.
    pub audio_id: qt_method!(fn(&self) -> QString),
    /// Apply core event (four call kinds).
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Ring limit ms; 0: app's own, a little over core's 120 s. Tests only.
    pub ring_limit_ms: qt_property!(u32),

    /// Platform back-camera ring labels ("1.0,0.5"). Empty if none.
    pub back_lenses: qt_property!(QString),

    /// Loopback host serving page, if any.
    host: Option<Host>,
    /// Offer for incoming call.
    offer: String,
    /// Counts calls: late answer for old call lands nowhere.
    generation: u64,
    /// Call events for account while outgoing call has no message yet: core's event can
    /// overtake host's report of placed id, and answer dropped for unknown call = call
    /// never connects. Replayed once id known; see `take_report`.
    early: Vec<(String, String)>,
    /// Core needs nothing more (it ended call, or other device took it). Host let go after
    /// (up now or still coming up) goes without `end_call`, which would end call on device
    /// that answered. One per call, so late host reads its own call's flag.
    settled: Arc<AtomicBool>,
    /// App in background.
    away: bool,
    /// Page keeps mic shut.
    mic_shut: bool,
}

/// Placed or answered, not over: sound is a call's that long (`CallAudio.qml`'s `talking`
/// holds audio policy set); see `call_host::label_call_audio`.
fn talking(state: &str) -> bool {
    !matches!(state, "" | "ringing" | "ended")
}

/// Ring time before rung out with no word from core. Core rings 120 s from send, then says
/// `CallEnded` once, from timer only in server that heard call. Overflowed event channel or
/// restarted server loses it: would ring until declined. A little over core's, so its word wins.
const RING_LIMIT: std::time::Duration = std::time::Duration::from_secs(125);

/// Max early events kept per call. Window is one core round trip; more aren't about placing.
const EARLY_EVENTS: usize = 16;

impl Call {
    /// Under way: any state short of over.
    fn busy(&self) -> bool {
        !matches!(self.state.to_string().as_str(), "" | "ended")
    }

    fn set_state(&mut self, state: &str) {
        let before = self.state.to_string();
        if before != state {
            if talking(state) != talking(&before) {
                call_host::label_call_audio(talking(state));
            }
            self.state = state.into();
            self.state_changed();
        }
    }

    /// New call: last forgotten, its answers turned away.
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
        self.camera = false;
        self.rear_camera = false;
        self.local_video = false;
        self.remote_video = false;
        self.local_painted = false;
        self.remote_painted = false;
        self.video_changed();
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
        self.camera = video;
        self.video_changed();
        self.set_state("starting");
        self.serve("startCall".to_string());
        true
    }

    /// Take up ringing call from row.
    pub fn pick_up(&mut self, account_id: u32, chat_id: u32, message_id: u32) {
        if self.busy() || account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        // Ask core: row lacks offer, may have stopped ringing.
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
            this_mut.camera = call.has_video;
            this_mut.video_changed();
            this_mut.set_state("ringing");
            this_mut.watch_ringing();
        });
        runtime.spawn(async move {
            done(summary(&rpc, account_id, message_id).await);
        });
    }

    /// Answer ringing call.
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
            // Core tells caller and other devices, says `CallEnded`;
            // arriving at ended call, not acted on twice.
            "ringing" => {
                self.end_call();
                self.finish("declined");
            }
            // Dropping host tells core (its `Drop`).
            _ => self.finish("hung-up"),
        }
    }

    /// Mic off or on. Page keeps mic state and tells other end, so it is told to press own
    /// switch. Page not up yet told once up (`serve`).
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

    /// Mic allowed or shut; kept across calls, as `CallAudio.qml` last said. Page not up yet
    /// told once up (see `serve`).
    pub fn set_microphone(&mut self, on: bool) {
        if self.mic_shut != on {
            return;
        }
        self.mic_shut = !on;
        self.tell(call_host::microphone_command(on));
    }

    /// Camera wanted, app in front.
    fn camera_live(&self) -> bool {
        self.camera && !self.away
    }

    /// Command to page if up. Page still coming up gets state as it then is (`serve`).
    fn tell(&self, command: String) {
        if let Some(host) = &self.host {
            host.command(command);
        }
    }

    /// Camera on or off.
    pub fn set_camera(&mut self, on: bool) {
        if !self.busy() || self.camera == on {
            return;
        }
        let before = self.camera_live();
        self.camera = on;
        self.video_changed();
        if self.camera_live() != before {
            self.tell(call_host::camera_command(self.camera_live()));
        }
    }

    /// Flip camera.
    pub fn flip_camera(&mut self) {
        if !self.busy() {
            return;
        }
        self.rear_camera = !self.rear_camera;
        self.video_changed();
        self.tell(call_host::facing_command(!self.rear_camera));
    }

    /// Audio policy id from `/proc/self/stat`.
    #[must_use]
    pub fn audio_id(&self) -> QString {
        std::fs::read_to_string("/proc/self/stat")
            .ok()
            .and_then(|stat| audio_app_id(&stat))
            .unwrap_or_default()
            .into()
    }

    /// App away or back.
    pub fn set_background(&mut self, away: bool) {
        if self.away == away {
            return;
        }
        let before = self.camera_live();
        self.away = away;
        self.tell(call_host::shown_command(!away));
        if self.camera_live() != before {
            self.tell(call_host::camera_command(self.camera_live()));
        }
    }

    /// Page's picture state.
    fn take_video(&mut self, video: call_host::Video) {
        if !self.busy() {
            return;
        }
        let mut changed = self.local_video != video.local
            || self.remote_video != video.remote
            || self.local_painted != video.local_painted
            || self.remote_painted != video.remote_painted
            || self.rear_camera == video.front;
        self.local_video = video.local;
        self.remote_video = video.remote;
        self.local_painted = video.local_painted;
        self.remote_painted = video.remote_painted;
        // Facing as page says: page turns back when other camera won't open.
        self.rear_camera = !video.front;
        // Camera that won't open switched off, as reader sees; not one let go for background.
        if video.failed && self.camera_live() {
            // Phone-only: written where log is read.
            eprintln!(
                "piirit: the camera would not open: {}",
                if video.error.is_empty() {
                    "no reason given"
                } else {
                    video.error.as_str()
                }
            );
            self.camera = false;
            changed = true;
        }
        if changed {
            self.video_changed();
        }
    }

    /// Forget an ended call.
    pub fn reset(&mut self) {
        if self.state.to_string() == "ended" {
            self.end_reason = QString::default();
            self.set_state("");
        }
    }

    /// Call over: page goes, reason said.
    fn finish(&mut self, reason: &str) {
        if self.close() {
            self.say_ended(reason);
        }
    }

    /// Ending: page goes, answers turned away. False if no call.
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
        if self.local_video || self.remote_video || self.local_painted || self.remote_painted {
            self.local_video = false;
            self.remote_video = false;
            self.local_painted = false;
            self.remote_painted = false;
            self.video_changed();
        }
        self.set_state("ended");
        true
    }

    /// Say why call ended.
    fn say_ended(&mut self, reason: &str) {
        self.end_reason = reason.into();
        self.state_changed();
        self.ended(reason.into());
    }

    /// Core needs nothing more; see `settled`.
    fn settle(&self) {
        self.settled.store(true, Ordering::SeqCst);
        if let Some(host) = &self.host {
            host.settle();
        }
    }

    /// Tell core to end call, no wait.
    fn end_call(&self) {
        let (account_id, message_id) = (self.account_id, self.message_id);
        if message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            // No wait, no answer to act on: ending reaches page as core's own event.
            rpc.call::<_, serde_json::Value>("end_call", (account_id, message_id))
                .await
                .ok();
        });
    }

    /// Other end's name and picture: chat's, which in 1:1 chat are theirs.
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

    /// Bring page up on `command`.
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

        // Host reports, raised on Qt thread.
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
            // Hung up while page coming up: dropping host lets it go, quietly (settled) if
            // core needs nothing more.
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
                        // Camera as now: may have switched while page came up.
                        let camera = this_mut.camera_live();
                        this_mut.url = host.url(page_options(camera), &command).into();
                        // Muted before page.
                        if this_mut.muted {
                            host.command(call_host::mute_command(true));
                        }
                        // Mic shut.
                        if this_mut.mic_shut {
                            host.command(call_host::microphone_command(false));
                        }
                        let back = main_back_camera(&this_mut.back_lenses.to_string());
                        if back > 0 {
                            host.command(call_host::back_camera_command(back));
                        }
                        // Turned or away before page.
                        if this_mut.rear_camera {
                            host.command(call_host::facing_command(false));
                        }
                        if this_mut.away {
                            host.command(call_host::shown_command(false));
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

    /// Apply host's report.
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
            Report::Video(video) => self.take_video(video),
            // Host told core.
            Report::EndedByPage => self.finish("hung-up"),
            Report::Failed(message) => {
                self.error(message.into());
                self.finish("failed");
            }
        }
    }

    /// Peer connection's ICE state.
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
            // ICE may recover; "failed" gave up.
            "disconnected" if self.state.to_string() == "connected" => {
                self.set_state("reconnecting");
            }
            "failed" => self.finish("failed"),
            _ => {}
        }
    }

    /// Apply core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        let kind = kind.to_string();
        if !matches!(
            kind.as_str(),
            "IncomingCall" | "IncomingCallAccepted" | "OutgoingCallAccepted" | "CallEnded"
        ) {
            return;
        }
        // Placing call, before core names it.
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
        // Call events only snake_case part of core stream: `msg_id`, not `msgId`.
        let Some(message_id) = json::u32_opt(&payload, "msg_id") else {
            return;
        };
        let ours = context_id == self.account_id && message_id == self.message_id;
        match kind.as_str() {
            "IncomingCall" => {
                // One at a time, as both reference clients: second rings nowhere, becomes
                // missed once core gives up.
                if self.busy() {
                    return;
                }
                let chat_id = json::u32_at(&payload, "chat_id");
                self.begin(context_id, chat_id, message_id, true);
                self.has_video = json::flag(&payload, "has_video");
                self.offer = json::str_at(&payload, "place_call_info").to_string();
                self.call_changed();
                self.camera = self.has_video;
                self.video_changed();
                self.set_state("ringing");
                self.watch_ringing();
                self.ringing();
            }
            // Answered here (host already reported) or on other device. Core says latter only
            // for call this device hasn't accepted, and voids own accept after; so short of
            // over, call is that device's: letting go must not end it there.
            "IncomingCallAccepted" if ours => {
                if !json::flag(&payload, "from_this_device") && self.busy() {
                    self.settle();
                    self.finish("answered-elsewhere");
                }
            }
            // Other end's answer to page. All account's devices hear it; only placer has page.
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
                // Core said so.
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

    /// Stop call ringing past [`RING_LIMIT`] with no word from core. Core counts it over by
    /// then whatever its timer did (stale 120 s after send, `call_info` says so): end as
    /// rung out, core asked which way.
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
            // Answered, declined, ended meanwhile: answering keeps call, ending starts new count.
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

    /// Ended unanswered: missed, or declined on other device. Core knows which; matters for
    /// notification. Ringing stops now, only reason waits for core.
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
            // Newer call: reason moot.
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

/// Host needs from core: ICE servers, other end's picture. Neither worth failing call:
/// unanswered question answered with nothing.
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
    use super::{audio_app_id, main_back_camera, talking, Summary};
    use serde_json::json;

    #[test]
    fn the_main_back_camera_is_the_one_labelled_nearest_one() {
        assert_eq!(main_back_camera("1.0,0.5"), 0);
        assert_eq!(main_back_camera("0.5,1.0"), 1);
        assert_eq!(main_back_camera("0.6, 1.0, 2.0"), 1);
        assert_eq!(main_back_camera("0.5x,1x"), 1);
        assert_eq!(main_back_camera("0.6,3.0"), 0);
        assert_eq!(main_back_camera("3.0,0.6"), 1);
        assert_eq!(main_back_camera("2.0,0.5"), 1);
        assert_eq!(main_back_camera("0.5,1.5"), 0);
        assert_eq!(main_back_camera(""), 0);
        assert_eq!(main_back_camera("wide,main"), 0);
        assert_eq!(main_back_camera("wide,1.0"), 1);
    }

    #[test]
    fn a_call_is_talking_from_placed_or_answered_until_over() {
        for state in [
            "starting",
            "calling",
            "connecting",
            "connected",
            "reconnecting",
        ] {
            assert!(talking(state), "{state}");
        }
        for state in ["", "ringing", "ended"] {
            assert!(!talking(state), "{state:?}");
        }
    }

    #[test]
    fn the_audio_id_is_the_start_time_in_hex() {
        let stat = "4242 (harbour-piirit) S 1 4242 4242 0 -1 4194560 2915 0 \
                    0 0 12 4 0 0 20 0 9 0 4660 917504000 9000 18446744073709551615 \
                    1 1 0 0 0 0 0 4096 17663 0 0 0 17 3 0 0 0 0 0\n";
        assert_eq!(audio_app_id(stat).as_deref(), Some("1234"));
        // Single-space split, as libpulse and libresource:
        // name with space shifts field for all three.
        let spaced = stat.replace("(harbour-piirit)", "(harbour piirit)");
        assert_eq!(audio_app_id(&spaced).as_deref(), Some("0"));
        assert_eq!(audio_app_id("4242 (piirit) S 1"), None);
        let garbled = stat.replace(" 4660 ", " x4660 ");
        assert_eq!(audio_app_id(&garbled), None);
    }

    #[test]
    fn a_call_reads_as_the_core_serialises_it() {
        // Shapes from deltachat-rpc-server 2.62: camelCase, state tagged by `kind`.
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
