//! Voice message transcription (experimental): settings, speech model, per-message state.
//! Engine: `piirit-speech` on own thread (`worker`). Store: `SQLite` beside accounts, indexed
//! for search later (`store`). Queue and live views: `hub`. Rules: `schedule`.

mod download;
mod files;
mod hub;
mod schedule;
mod store;
mod worker;

use std::path::Path;

use qmetaobject::*;

use download::{Fetch, FetchEvent, Hasher};
use files::ModelSpec;
use hub::Shown;
use schedule::Conditions;

pub(crate) use hub::connected;

/// No transcript, nothing queued.
pub const STATE_NONE: u32 = 0;
/// Queued; waits for its turn, the charger, or the model.
pub const STATE_QUEUED: u32 = 1;
/// Running; `permille` and `text` grow.
pub const STATE_RUNNING: u32 = 2;
/// Done; `text`, `language` set.
pub const STATE_DONE: u32 = 3;
/// Nothing spoken.
pub const STATE_NO_SPEECH: u32 = 4;
/// Spoken language not selected; `language` = detected.
pub const STATE_OTHER_LANGUAGE: u32 = 5;
/// Failed; `error` set.
pub const STATE_FAILED: u32 = 6;

/// Speech model not on phone.
pub const MODEL_ABSENT: u32 = 0;
/// Downloading; `model_permille` grows.
pub const MODEL_DOWNLOADING: u32 = 1;
/// On phone, checked.
pub const MODEL_READY: u32 = 2;
/// Download or check failed; `model_error` set.
pub const MODEL_FAILED: u32 = 3;

/// App-wide side: settings in, model state out. One instance (`qml/piirit.qml`).
///
/// ```qml
/// Transcriber {
///     id: transcriber
///     enabled: Settings.transcriptionEnabled
///     languages: Settings.transcriptionLanguages
///     mode: Settings.transcriptionMode
///     older: Settings.transcriptionOlder
///     charging: power.charging
///     power_saving: power.saving
/// }
/// ```
// One flag per Qt property: QML binds each.
#[allow(clippy::struct_excessive_bools)]
#[derive(QObject, Default)]
pub struct Transcriber {
    base: qt_base_class!(trait QObject),

    /// Feature switch. Off: model deleted, transcripts kept, nothing runs.
    pub enabled: qt_property!(bool; WRITE set_enabled NOTIFY settings_changed),
    /// Whisper codes, comma-separated (`en,fi`).
    pub languages: qt_property!(QString; WRITE set_languages NOTIFY settings_changed),
    /// 0 never (tap only), 1 while charging, 2 always (on download).
    pub mode: qt_property!(u32; WRITE set_mode NOTIFY settings_changed),
    /// Older voice messages too, while charging.
    pub older: qt_property!(bool; WRITE set_older NOTIFY settings_changed),
    /// Emitted when a setting changes.
    pub settings_changed: qt_signal!(),

    /// Charger in (MCE, from QML).
    pub charging: qt_property!(bool; WRITE set_charging NOTIFY power_changed),
    /// Battery saving mode on (MCE, from QML). Pauses automatic jobs.
    pub power_saving: qt_property!(bool; WRITE set_power_saving NOTIFY power_changed),
    /// Emitted when power state changes.
    pub power_changed: qt_signal!(),

    /// `MODEL_*`.
    pub model_state: qt_property!(u32; NOTIFY model_changed),
    /// Download progress, 0..=1000.
    pub model_permille: qt_property!(u32; NOTIFY model_changed),
    /// Why download or check failed.
    pub model_error: qt_property!(QString; NOTIFY model_changed),
    /// Emitted when model state changes.
    pub model_changed: qt_signal!(),
    /// Fetch model (resumes partial file). Starts on its own when enabled with none on phone.
    pub download_model: qt_method!(fn(&mut self)),
    /// Stop download; partial file kept for resume.
    pub cancel_download: qt_method!(fn(&mut self)),

    /// Count voice messages in chat without transcript; answer via `chat_counted`.
    pub count_chat: qt_method!(fn(&mut self, account_id: u32, chat_id: u32)),
    /// Answer to `count_chat`.
    pub chat_counted: qt_signal!(account_id: u32, chat_id: u32, count: u32),
    /// Queue every voice message in chat without transcript, as if tapped.
    pub transcribe_chat: qt_method!(fn(&mut self, account_id: u32, chat_id: u32)),

    /// Core event, wired from QML like other models.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Known to hub, model checked.
    attached: bool,
    /// Download running.
    fetch: Option<Fetch>,
    /// Part being hashed before resume.
    hashing: bool,
    /// Download count: late answers of a stopped one dropped.
    generation: u64,
    /// Download started by itself since switched on.
    auto_started: bool,
}

impl Transcriber {
    /// Set switch. Off: model deleted, queue dropped, transcripts hidden.
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            if enabled {
                self.auto_started = false;
            } else {
                self.remove_model();
            }
            self.settings_changed();
            self.settle();
        }
    }

    /// Set languages.
    pub fn set_languages(&mut self, languages: QString) {
        if self.languages != languages {
            self.languages = languages;
            self.settings_changed();
            self.settle();
        }
    }

    /// Set mode.
    pub fn set_mode(&mut self, mode: u32) {
        if self.mode != mode {
            self.mode = mode;
            self.settings_changed();
            self.settle();
        }
    }

    /// Set backlog switch.
    pub fn set_older(&mut self, older: bool) {
        if self.older != older {
            self.older = older;
            self.settings_changed();
            self.settle();
        }
    }

    /// Set charger state.
    pub fn set_charging(&mut self, charging: bool) {
        if self.charging != charging {
            self.charging = charging;
            self.power_changed();
            self.settle();
        }
    }

    /// Set battery saving state.
    pub fn set_power_saving(&mut self, power_saving: bool) {
        if self.power_saving != power_saving {
            self.power_saving = power_saving;
            self.power_changed();
            self.settle();
        }
    }

    /// After any setting: known to hub, model fetched once if missing, hub told.
    fn settle(&mut self) {
        if !self.attached {
            self.attached = true;
            hub::attach(QPointer::from(&*self));
            self.check_model();
        }
        if self.enabled && self.model_state == MODEL_ABSENT && !self.auto_started {
            self.auto_started = true;
            self.download_model();
        }
        hub::set_conditions(
            Conditions {
                enabled: self.enabled,
                model_ready: self.model_state == MODEL_READY,
                mode: self.mode,
                older: self.older,
                charging: self.charging,
                power_saving: self.power_saving,
            },
            self.languages
                .to_string()
                .split(',')
                .map(str::trim)
                .filter(|code| !code.is_empty())
                .map(str::to_owned)
                .collect(),
        );
    }

    /// Model on phone at right size: ready. Hashed only after download, not every start.
    fn check_model(&mut self) {
        let size = files::model_path()
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|meta| meta.len());
        if size == Some(ModelSpec::current().bytes) {
            self.set_model(MODEL_READY, 1000, String::new());
        }
    }

    fn set_model(&mut self, state: u32, permille: u32, error: String) {
        let error = QString::from(error);
        if self.model_state == state && self.model_permille == permille && self.model_error == error
        {
            return;
        }
        self.model_state = state;
        self.model_permille = permille;
        self.model_error = error;
        self.model_changed();
    }

    /// Fetch model (resumes partial file).
    pub fn download_model(&mut self) {
        if !self.enabled || self.fetch.is_some() || self.hashing || self.model_state == MODEL_READY
        {
            return;
        }
        let spec = ModelSpec::current();
        let model = match files::make_speech_dir() {
            Ok(dir) => dir.join(files::MODEL_FILE),
            Err(err) => {
                self.fail_model(err);
                return;
            }
        };
        let part = files::partial_path(&model);
        let have = std::fs::metadata(&part).map_or(0, |meta| meta.len());
        self.generation = self.generation.wrapping_add(1);
        self.set_model(MODEL_DOWNLOADING, permille(have, spec.bytes), String::new());
        if have == 0 {
            self.begin(&spec, &part, None);
            return;
        }
        // Part hashed off Qt thread (190 MB: seconds), then fetch goes on from its end.
        self.hashing = true;
        let generation = self.generation;
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let hashed = queued_callback(move |hash: Hasher| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation == generation {
                this.borrow_mut().resume(hash);
            }
        });
        let started = std::thread::Builder::new()
            .name("piirit-model-hash".to_owned())
            .spawn(move || hashed(Hasher::of_file(&part)));
        if let Err(err) = started {
            self.hashing = false;
            self.fail_model(format!("could not check the partial download: {err}"));
        }
    }

    /// Part hashed: fetch rest, or check it if whole.
    fn resume(&mut self, hash: Hasher) {
        self.hashing = false;
        if !self.enabled || self.model_state != MODEL_DOWNLOADING {
            return;
        }
        let spec = ModelSpec::current();
        let Some(model) = files::model_path() else {
            return;
        };
        if hash.bytes() >= spec.bytes {
            self.verify(&spec, &model, &hash.hex());
        } else {
            self.begin(&spec, &files::partial_path(&model), Some(hash));
        }
    }

    fn begin(&mut self, spec: &ModelSpec, part: &Path, hash: Option<Hasher>) {
        let generation = self.generation;
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let report = queued_callback(move |event: FetchEvent| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation == generation {
                this.borrow_mut().fetched(event);
            }
        });
        match Fetch::start(&spec.url, part, spec.bytes, hash, report) {
            Ok(fetch) => self.fetch = Some(fetch),
            Err(err) => self.fail_model(err),
        }
    }

    fn fetched(&mut self, event: FetchEvent) {
        let spec = ModelSpec::current();
        match event {
            FetchEvent::Progress(bytes) => {
                if self.model_state == MODEL_DOWNLOADING {
                    self.set_model(
                        MODEL_DOWNLOADING,
                        permille(bytes, spec.bytes),
                        String::new(),
                    );
                }
            }
            FetchEvent::Done(result) => {
                self.fetch = None;
                match (result, files::model_path()) {
                    (Ok(hex), Some(model)) => self.verify(&spec, &model, &hex),
                    (Ok(_), None) => self.fail_model("no place for the model".to_owned()),
                    (Err(err), _) => self.fail_model(err),
                }
            }
        }
    }

    /// Whole part: right size and hash moves into place; else deleted (can't resume wrong one).
    fn verify(&mut self, spec: &ModelSpec, model: &Path, hex: &str) {
        let part = files::partial_path(model);
        let size = std::fs::metadata(&part).map_or(0, |meta| meta.len());
        if size != spec.bytes || !hex.eq_ignore_ascii_case(&spec.sha256) {
            remove(&part);
            self.fail_model(format!(
                "the downloaded model is damaged ({size} bytes, SHA-256 {hex})"
            ));
            return;
        }
        if let Err(err) = std::fs::rename(&part, model) {
            self.fail_model(format!("could not keep the model: {err}"));
            return;
        }
        self.set_model(MODEL_READY, 1000, String::new());
        self.settle();
    }

    fn fail_model(&mut self, error: String) {
        eprintln!("piirit: speech model: {error}");
        self.set_model(MODEL_FAILED, self.model_permille, error);
    }

    /// Stop download; partial file kept for resume.
    pub fn cancel_download(&mut self) {
        if self.fetch.is_none() && !self.hashing {
            return;
        }
        self.fetch = None;
        self.hashing = false;
        self.generation = self.generation.wrapping_add(1);
        self.set_model(MODEL_ABSENT, 0, String::new());
    }

    /// Feature off: download stopped, model and part deleted.
    fn remove_model(&mut self) {
        self.fetch = None;
        self.hashing = false;
        self.generation = self.generation.wrapping_add(1);
        if let Some(model) = files::model_path() {
            remove(&files::partial_path(&model));
            remove(&model);
        }
        self.set_model(MODEL_ABSENT, 0, String::new());
    }

    /// Count chat's voice messages without transcript.
    pub fn count_chat(&mut self, account_id: u32, chat_id: u32) {
        if !hub::count_chat(account_id, chat_id) {
            self.chat_counted(account_id, chat_id, 0);
        }
    }

    /// Queue chat's voice messages.
    pub fn transcribe_chat(&mut self, account_id: u32, chat_id: u32) {
        hub::transcribe_chat(account_id, chat_id);
    }

    /// Core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        let msg = crate::json::u32_at(&payload, "msgId");
        match kind.to_string().as_str() {
            // Arrival; `MsgsChanged` also when a limited download lands.
            "IncomingMsg" | "MsgsChanged" if msg != 0 => hub::arrived(context_id, msg),
            "MsgDeleted" if msg != 0 => hub::deleted(context_id, msg),
            "AccountsChanged" => hub::accounts_changed(),
            _ => {}
        }
    }
}

/// Progress, 0..=1000.
fn permille(have: u64, total: u64) -> u32 {
    have.min(total)
        .saturating_mul(1000)
        .checked_div(total)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

/// Delete if there.
fn remove(path: &Path) {
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            eprintln!("piirit: could not delete a speech file: {err}");
        }
        _ => {}
    }
}

/// One voice message's transcript, for its bubble or Audio row.
///
/// ```qml
/// Transcript {
///     id: transcript
///     account_id: page.accountId
///     message_id: model.id
/// }
/// Label { text: transcript.text; visible: transcript.state === 3 }
/// ```
#[derive(QObject, Default)]
pub struct Transcript {
    base: qt_base_class!(trait QObject),

    /// Account of message. Setting reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY source_changed),
    /// Message; 0 shows nothing. Setting reloads.
    pub message_id: qt_property!(u32; WRITE set_message_id NOTIFY source_changed),
    /// Emitted when target changes.
    pub source_changed: qt_signal!(),

    /// `STATE_*`.
    pub state: qt_property!(u32; NOTIFY changed),
    /// Queued or run by a tap: charger and battery saving don't hold it back.
    pub manual: qt_property!(bool; NOTIFY changed),
    /// Running: 0..=1000.
    pub permille: qt_property!(u32; NOTIFY changed),
    /// Done: transcript. Running: text so far.
    pub text: qt_property!(QString; NOTIFY changed),
    /// Done or other language: whisper code.
    pub language: qt_property!(QString; NOTIFY changed),
    /// Failed: why.
    pub error: qt_property!(QString; NOTIFY changed),
    /// Emitted when any of the above change.
    pub changed: qt_signal!(),

    /// Queue now, ahead of automatic jobs. `any_language`: skip language check.
    pub transcribe: qt_method!(fn(&mut self, any_language: bool)),
    /// Drop from queue or stop run.
    pub cancel: qt_method!(fn(&mut self)),

    /// Key registered with hub.
    watched: (u32, u32),
}

impl Transcript {
    /// Set account.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.source_changed();
            self.watch();
        }
    }

    /// Set message.
    pub fn set_message_id(&mut self, message_id: u32) {
        if self.message_id != message_id {
            self.message_id = message_id;
            self.source_changed();
            self.watch();
        }
    }

    /// Follow new message: old state off now, new one soon.
    fn watch(&mut self) {
        let key = (self.account_id, self.message_id);
        hub::watch(&QPointer::from(&*self), self.watched, key);
        self.watched = key;
        self.show(&Shown::default());
    }

    /// Queue now.
    pub fn transcribe(&mut self, any_language: bool) {
        hub::transcribe((self.account_id, self.message_id), any_language);
    }

    /// Drop or stop.
    pub fn cancel(&mut self) {
        hub::cancel((self.account_id, self.message_id));
    }

    /// Take hub's state.
    fn show(&mut self, shown: &Shown) {
        let (text, language, error) = (
            QString::from(shown.text.as_str()),
            QString::from(shown.language.as_str()),
            QString::from(shown.error.as_str()),
        );
        if self.state == shown.state
            && self.manual == shown.manual
            && self.permille == shown.permille
            && self.text == text
            && self.language == language
            && self.error == error
        {
            return;
        }
        self.state = shown.state;
        self.manual = shown.manual;
        self.permille = shown.permille;
        self.text = text;
        self.language = language;
        self.error = error;
        self.changed();
    }
}
