//! Voice message to text, on phone: [`decode`] reads file (FFmpeg), [`Engine`] transcribes
//! (whisper.cpp). Nothing leaves process.
//!
//! Interface fixed up front; shim codes against it. Engine body lands with `build.rs`.

#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects,
        clippy::let_underscore_must_use,
        clippy::panic,
        clippy::panic_in_result_fn,
        clippy::unwrap_in_result,
        clippy::get_unwrap
    )
)]

use std::path::Path;

/// Rate [`decode`] returns and [`Engine::transcribe`] takes.
pub const SAMPLE_RATE: u32 = 16_000;

/// Why a step failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Progress callback asked to stop.
    Cancelled,
    /// Engine library missing or unloadable.
    Unavailable(String),
    /// Model file unreadable or not a whisper model.
    Model(String),
    /// Audio undecodable.
    Audio(String),
    /// Engine failed mid-run.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Unavailable(why) => write!(f, "speech engine unavailable: {why}"),
            Self::Model(why) => write!(f, "speech model: {why}"),
            Self::Audio(why) => write!(f, "audio: {why}"),
            Self::Failed(why) => write!(f, "transcription failed: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// One run's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Whisper codes (`en`, `fi`) speech may be in; empty = any.
    pub languages: Vec<String>,
    /// Cores to run on, by id; empty = any. [`big_cores`] gives fastest.
    pub cpus: Vec<u32>,
    /// Idle priority for this run's threads: background jobs yield to UI.
    pub background: bool,
}

/// Run so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// 0..=1000.
    pub permille: u16,
    /// Text so far.
    pub text: String,
}

/// What a run found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Speech as text. `language`: whisper code.
    Text {
        /// Whisper code.
        language: String,
        /// Transcript.
        text: String,
    },
    /// Nothing spoken.
    NoSpeech,
    /// Spoken language not in [`Options::languages`]; nothing transcribed.
    OtherLanguage {
        /// Whisper code detected.
        language: String,
    },
}

/// Loaded model. One run at a time; drop frees its memory.
#[derive(Debug)]
pub struct Engine {
    _private: (),
}

impl Engine {
    /// Loads `model` (ggml whisper file). `libs`: dir with engine libraries.
    ///
    /// # Errors
    ///
    /// [`Error::Unavailable`], [`Error::Model`].
    pub fn load(_libs: &Path, _model: &Path) -> Result<Self, Error> {
        Err(Error::Unavailable("not built yet".into()))
    }

    /// Transcribes [`SAMPLE_RATE`] mono samples. `progress` returning false cancels.
    ///
    /// # Errors
    ///
    /// [`Error::Cancelled`], [`Error::Failed`].
    pub fn transcribe(
        &mut self,
        _samples: &[f32],
        _options: &Options,
        _progress: &mut dyn FnMut(&Progress) -> bool,
    ) -> Result<Outcome, Error> {
        Err(Error::Unavailable("not built yet".into()))
    }
}

/// Decodes voice file (MP3, AAC, M4A, Opus, Vorbis, WAV) to [`SAMPLE_RATE`] mono, at most
/// `max_seconds`.
///
/// # Errors
///
/// [`Error::Audio`].
pub fn decode(_path: &Path, _max_seconds: u32) -> Result<Vec<f32>, Error> {
    Err(Error::Unavailable("not built yet".into()))
}

/// Ids of fastest cores (highest `cpuinfo_max_freq`); empty if unknown.
#[must_use]
pub fn big_cores() -> Vec<u32> {
    Vec::new()
}
