//! Voice message transcription (experimental): settings, speech model, per-message state.
//! Engine: `piirit-speech`. Store: `SQLite` beside accounts, built for search later.
//!
//! Interface fixed up front; QML codes against it. Bodies land with store and scheduler.

use qmetaobject::*;

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

/// App-wide side: settings in, model and counts out. One instance (`qml/piirit.qml`).
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
    /// Model name for display (`Whisper small`).
    pub model_name: qt_property!(QString; CONST),
    /// Download size, bytes, for "needs N MB".
    pub model_download_bytes: qt_property!(f64; CONST),
    /// Model size on phone, bytes; 0 if absent.
    pub model_bytes: qt_property!(f64; NOTIFY model_changed),
    /// Why download or check failed.
    pub model_error: qt_property!(QString; NOTIFY model_changed),
    /// Emitted when model state changes.
    pub model_changed: qt_signal!(),
    /// Fetch model (resumes partial file). Starts on its own when enabled with none on phone.
    pub download_model: qt_method!(fn(&mut self)),
    /// Stop download; partial file kept for resume.
    pub cancel_download: qt_method!(fn(&mut self)),

    /// Transcripts stored, all profiles.
    pub transcript_count: qt_property!(u32; NOTIFY store_changed),
    /// Their size, bytes.
    pub transcript_bytes: qt_property!(f64; NOTIFY store_changed),
    /// Emitted when counts change.
    pub store_changed: qt_signal!(),
    /// Delete every transcript.
    pub delete_all: qt_method!(fn(&mut self)),

    /// Count voice messages in chat without transcript; answer via `chat_counted`.
    pub count_chat: qt_method!(fn(&mut self, account_id: u32, chat_id: u32)),
    /// Answer to `count_chat`.
    pub chat_counted: qt_signal!(account_id: u32, chat_id: u32, count: u32),
    /// Queue every voice message in chat without transcript, as if tapped.
    pub transcribe_chat: qt_method!(fn(&mut self, account_id: u32, chat_id: u32)),

    /// Core event, wired from QML like other models.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),
}

impl Transcriber {
    /// Set switch.
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.settings_changed();
        }
    }

    /// Set languages.
    pub fn set_languages(&mut self, languages: QString) {
        if self.languages != languages {
            self.languages = languages;
            self.settings_changed();
        }
    }

    /// Set mode.
    pub fn set_mode(&mut self, mode: u32) {
        if self.mode != mode {
            self.mode = mode;
            self.settings_changed();
        }
    }

    /// Set backlog switch.
    pub fn set_older(&mut self, older: bool) {
        if self.older != older {
            self.older = older;
            self.settings_changed();
        }
    }

    /// Set charger state.
    pub fn set_charging(&mut self, charging: bool) {
        if self.charging != charging {
            self.charging = charging;
            self.power_changed();
        }
    }

    /// Set battery saving state.
    pub fn set_power_saving(&mut self, power_saving: bool) {
        if self.power_saving != power_saving {
            self.power_saving = power_saving;
            self.power_changed();
        }
    }

    /// Fetch model.
    pub fn download_model(&mut self) {}

    /// Stop download.
    pub fn cancel_download(&mut self) {}

    /// Delete every transcript.
    pub fn delete_all(&mut self) {}

    /// Count chat's voice messages without transcript.
    pub fn count_chat(&mut self, account_id: u32, chat_id: u32) {
        self.chat_counted(account_id, chat_id, 0);
    }

    /// Queue chat's voice messages.
    pub fn transcribe_chat(&mut self, _account_id: u32, _chat_id: u32) {}

    /// Core event.
    pub fn handle_event(&mut self, _context_id: u32, _kind: QString, _payload_json: QString) {}
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
}

impl Transcript {
    /// Set account.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.source_changed();
        }
    }

    /// Set message.
    pub fn set_message_id(&mut self, message_id: u32) {
        if self.message_id != message_id {
            self.message_id = message_id;
            self.source_changed();
        }
    }

    /// Queue now.
    pub fn transcribe(&mut self, _any_language: bool) {}

    /// Drop or stop.
    pub fn cancel(&mut self) {}
}
