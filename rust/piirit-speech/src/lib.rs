//! Voice message to text, on phone: [`decode`] reads file (FFmpeg), [`Engine`] transcribes
//! (whisper.cpp). Nothing leaves process.
//!
//! Engine = shared lib loaded at run time (`build.rs`): one per CPU level, picked by what
//! this CPU does. Runs on a thread of its own: pinned and lowered there, never caller's.

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

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

/// Rate [`decode`] returns and [`Engine::transcribe`] takes.
pub const SAMPLE_RATE: u32 = 16_000;

/// Engine libs `build.rs` made, for tests: `target/<profile>/speech/` holds same.
#[doc(hidden)]
pub const BUILT_LIBS: &str = env!("PIIRIT_SPEECH_LIBS");

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
pub struct Engine {
    speech: NonNull<c_void>,
    library: Library,
}

// SAFETY: whisper.cpp context has no thread affinity; `&mut self` keeps use to one thread
// at a time.
#[allow(unsafe_code)]
unsafe impl Send for Engine {}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").finish_non_exhaustive()
    }
}

impl Engine {
    /// Loads `model` (ggml whisper file). `libs`: dir with engine libraries.
    ///
    /// # Errors
    ///
    /// [`Error::Unavailable`], [`Error::Model`].
    pub fn load(libs: &Path, model: &Path) -> Result<Self, Error> {
        let library = Library::open(&libs.join(library_name()))?;
        let model = c_path(model).map_err(Error::Model)?;
        let mut error = [0; ERROR_LEN];
        // SAFETY: `load` matches speech.h; path NUL-terminated and error buffer's length
        // beside it, both outlive call.
        #[allow(unsafe_code)]
        let speech = unsafe { (library.load)(model.as_ptr(), error.as_mut_ptr(), ERROR_LEN) };
        let speech = NonNull::new(speech).ok_or_else(|| Error::Model(message(&error)))?;
        Ok(Self { speech, library })
    }

    /// Transcribes [`SAMPLE_RATE`] mono samples. `progress` returning false cancels.
    ///
    /// # Errors
    ///
    /// [`Error::Cancelled`], [`Error::Failed`].
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        options: &Options,
        progress: &mut dyn FnMut(&Progress) -> bool,
    ) -> Result<Outcome, Error> {
        let n_samples = i32::try_from(samples.len())
            .map_err(|err| Error::Failed(format!("sound too long: {err}")))?;
        let cpus: Vec<i32> = options
            .cpus
            .iter()
            .filter_map(|&cpu| i32::try_from(cpu).ok())
            .collect();
        let shape = Shape {
            threads: thread_count(&options.cpus, big_cores),
            audio_ctx: audio_context(samples.len()),
            background: options.background,
        };
        let cancel = AtomicBool::new(false);
        let (events, received) = mpsc::channel();

        let job = Job {
            samples,
            n_samples,
            cpus: &cpus,
            shape,
        };
        let (code, spoken, stop, error, text) = std::thread::scope(|scope| {
            let speech: &mut Self = self;
            let cancel = &cancel;
            let job = &job;
            let worker = std::thread::Builder::new()
                .name("piirit-speech".into())
                .spawn_scoped(scope, move || {
                    let mut hooks = Hooks {
                        events,
                        cancel,
                        selected: &options.languages,
                        one_window: samples.len() <= WINDOW_POSITIONS * SAMPLES_PER_POSITION,
                        stop: None,
                    };
                    let (code, spoken, error) = speech.run(job, &mut hooks);
                    (code, spoken, hooks.stop, error)
                })
                .map_err(|err| Error::Failed(format!("no thread for the engine: {err}")))?;
            let mut now = Progress::default();
            let mut text = String::new();
            for event in received {
                match event {
                    Event::Progress(permille) => now.permille = permille,
                    Event::Segment(segment) => {
                        text.push_str(&segment);
                        text.trim().clone_into(&mut now.text);
                    }
                }
                if !cancel.load(Ordering::Relaxed) && !progress(&now) {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            let (code, spoken, stop, error) = worker.join().map_err(|panic| {
                let why = panic
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("unknown");
                Error::Failed(format!("the engine's thread panicked: {why}"))
            })?;
            Ok((code, spoken, stop, error, text))
        })?;
        if cancel.load(Ordering::Relaxed) || code == ffi::CANCELLED {
            return Err(Error::Cancelled);
        }
        match code {
            ffi::OK => {
                let text = text.trim();
                Ok(if text.is_empty() {
                    Outcome::NoSpeech
                } else {
                    Outcome::Text {
                        language: spoken,
                        text: text.to_owned(),
                    }
                })
            }
            ffi::STOPPED => match stop {
                Some(Stop::Other(language)) => Ok(Outcome::OtherLanguage { language }),
                Some(Stop::Silent) => Ok(Outcome::NoSpeech),
                Some(Stop::Unknown) => Err(Error::Failed(format!(
                    "unknown language in {:?}",
                    options.languages
                ))),
                None => Err(Error::Failed("no language detected".into())),
            },
            _ => Err(Error::Failed(error)),
        }
    }

    /// One run on calling thread: what C returned, spoken language, error.
    fn run(&self, job: &Job<'_>, hooks: &mut Hooks<'_>) -> (i32, String, String) {
        let raw = ffi::Job {
            samples: job.samples.as_ptr(),
            n_samples: job.n_samples,
            cpus: job.cpus.as_ptr(),
            n_cpus: i32::try_from(job.cpus.len()).unwrap_or(0),
            threads: job.shape.threads,
            background: i32::from(job.shape.background),
            audio_ctx: job.shape.audio_ctx,
        };
        let callbacks = ffi::Hooks {
            context: std::ptr::addr_of_mut!(*hooks).cast::<c_void>(),
            choose: on_choose,
            progress: on_progress,
            segment: on_segment,
            cancelled: on_cancelled,
        };
        let mut spoken = [0; LANGUAGE_LEN];
        let mut error = [0; ERROR_LEN];
        // SAFETY: `run` matches speech.h; `speech` from this library's `load`, not freed;
        // `raw`'s pointers into `job`, `callbacks`' context is `hooks`, buffers with
        // lengths: all outlive call, C keeps none.
        #[allow(unsafe_code)]
        let code = unsafe {
            (self.library.run)(
                self.speech.as_ptr(),
                &raw,
                &callbacks,
                spoken.as_mut_ptr(),
                LANGUAGE_LEN,
                error.as_mut_ptr(),
                ERROR_LEN,
            )
        };
        (code, message(&spoken), message(&error))
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // SAFETY: `speech` from this library's `load`, freed once here; library closes
        // after (field drop).
        #[allow(unsafe_code)]
        unsafe {
            (self.library.free)(self.speech.as_ptr());
        }
    }
}

/// Decodes voice file (MP3, AAC, M4A, Opus, Vorbis, WAV) to [`SAMPLE_RATE`] mono, at most
/// `max_seconds`.
///
/// # Errors
///
/// [`Error::Audio`].
pub fn decode(path: &Path, max_seconds: u32) -> Result<Vec<f32>, Error> {
    let most = u64::from(max_seconds).saturating_mul(u64::from(SAMPLE_RATE));
    piirit_video::sound(path, SAMPLE_RATE, most).map_err(Error::Audio)
}

/// Ids of fastest cores (highest `cpuinfo_max_freq`); empty if unknown.
#[must_use]
pub fn big_cores() -> Vec<u32> {
    big_cores_in(Path::new("/sys/devices/system/cpu"))
}

/// [`big_cores`] under `root` (sysfs cpu dir). Unreadable cores left out.
fn big_cores_in(root: &Path) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let cores: Vec<(u32, u64)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let id = name.to_str()?.strip_prefix("cpu")?.parse().ok()?;
            let freq = std::fs::read_to_string(entry.path().join("cpufreq/cpuinfo_max_freq"))
                .ok()?
                .trim()
                .parse()
                .ok()?;
            Some((id, freq))
        })
        .collect();
    let Some(fastest) = cores.iter().map(|&(_, freq)| freq).max() else {
        return Vec::new();
    };
    let mut big: Vec<u32> = cores
        .into_iter()
        .filter(|&(_, freq)| freq == fastest)
        .map(|(id, _)| id)
        .collect();
    big.sort_unstable();
    big
}

/// Engine threads: one per given core, else per big core, else 4. At most 8 (phones'
/// count; more only adds thread start-up per step).
fn thread_count(cpus: &[u32], big: impl FnOnce() -> Vec<u32>) -> i32 {
    let count = if cpus.is_empty() {
        match big().len() {
            0 => 4,
            n => n,
        }
    } else {
        cpus.len()
    };
    i32::try_from(count.min(8)).unwrap_or(4)
}

/// Samples per encoder position: whisper encodes 50 per second, 1500 per 30 s window.
const SAMPLES_PER_POSITION: usize = 320;
const WINDOW_POSITIONS: usize = 1500;

/// Encoder positions for a clip: its length plus ~2.5 s, in 64s; 0 (model's 1500) past one
/// window. Past 30 s whisper.cpp moves window 30 s per step, whatever this says: smaller
/// would skip sound.
fn audio_context(samples: usize) -> i32 {
    let positions = samples.div_ceil(SAMPLES_PER_POSITION);
    if positions > WINDOW_POSITIONS {
        return 0;
    }
    let generous = positions
        .saturating_add(128)
        .div_ceil(64)
        .saturating_mul(64);
    i32::try_from(generous.min(WINDOW_POSITIONS)).unwrap_or(0)
}

/// Whisper's no-speech probability over this = silence (whisper's own `no_speech_thold`).
const NO_SPEECH_ABOVE: f32 = 0.6;

/// What detection over first window says; indices into probabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Decision {
    /// Transcribe in it.
    Speak(usize),
    /// Most likely language, not selected.
    Other(usize),
    /// Nothing spoken.
    Silent,
    /// Detection gave no languages.
    Unknown,
}

/// Language to transcribe in, or why not. `no_speech` decides only if clip fits first
/// window: past it, speech may come later.
fn decide(
    selected: &[String],
    detected: &[(&str, f32)],
    no_speech: f32,
    one_window: bool,
) -> Decision {
    if one_window && no_speech > NO_SPEECH_ABOVE {
        return Decision::Silent;
    }
    if selected.is_empty() {
        return likeliest(detected, |_| true).map_or(Decision::Unknown, Decision::Speak);
    }
    choose_language(selected, detected)
}

/// Most likely selected language, unless an unselected one is over twice as likely: then
/// that one, as [`Decision::Other`].
fn choose_language(selected: &[String], detected: &[(&str, f32)]) -> Decision {
    let Some(top) = likeliest(detected, |_| true) else {
        return Decision::Unknown;
    };
    match likeliest(detected, |code| {
        selected.iter().any(|wanted| wanted == code)
    }) {
        Some(wanted) if likelihood(detected, top) <= 2.0 * likelihood(detected, wanted) => {
            Decision::Speak(wanted)
        }
        _ => Decision::Other(top),
    }
}

/// Likeliest language `keep` accepts; first of equals.
fn likeliest(detected: &[(&str, f32)], keep: impl Fn(&str) -> bool) -> Option<usize> {
    (0..detected.len())
        .filter(|&index| detected.get(index).is_some_and(|&(code, _)| keep(code)))
        .fold(None, |best, index| match best {
            Some(best) if likelihood(detected, best) >= likelihood(detected, index) => Some(best),
            _ => Some(index),
        })
}

/// Unreadable probability counts as 0.
fn likelihood(detected: &[(&str, f32)], index: usize) -> f32 {
    detected
        .get(index)
        .map_or(0.0, |&(_, p)| if p.is_nan() { 0.0 } else { p })
}

/// Run's size and manners.
#[derive(Clone, Copy)]
struct Shape {
    threads: i32,
    audio_ctx: i32,
    background: bool,
}

/// What engine's thread is given.
struct Job<'a> {
    samples: &'a [f32],
    n_samples: i32,
    cpus: &'a [i32],
    shape: Shape,
}

/// From engine's thread to caller's.
enum Event {
    Progress(u16),
    Segment(String),
}

/// Why engine stopped before transcribing.
enum Stop {
    Other(String),
    Silent,
    Unknown,
}

/// C hooks' context, on engine's thread.
struct Hooks<'a> {
    events: mpsc::Sender<Event>,
    cancel: &'a AtomicBool,
    selected: &'a [String],
    /// Clip within whisper's first 30 s window.
    one_window: bool,
    stop: Option<Stop>,
}

/// Hook context back from C.
fn hooks<'a>(context: *mut c_void) -> &'a mut Hooks<'a> {
    // SAFETY: `context` is `Engine::run`'s `&mut Hooks`, alive and unborrowed for the whole
    // C call, which alone calls hooks, on its own thread.
    #[allow(unsafe_code)]
    unsafe {
        &mut *context.cast::<Hooks<'a>>()
    }
}

extern "C" fn on_choose(
    context: *mut c_void,
    codes: *const *const c_char,
    probs: *const f32,
    count: i32,
    no_speech: f32,
) -> i32 {
    let hooks = hooks(context);
    let count = usize::try_from(count).unwrap_or(0);
    if codes.is_null() || probs.is_null() || count == 0 {
        return -1;
    }
    // SAFETY: C passes `count` codes (NUL-terminated, static in engine) and `count`
    // probabilities, valid during call.
    #[allow(unsafe_code)]
    let detected: Vec<(&str, f32)> = unsafe {
        std::slice::from_raw_parts(codes, count)
            .iter()
            .zip(std::slice::from_raw_parts(probs, count))
            .map(|(&code, &p)| {
                let code = if code.is_null() {
                    ""
                } else {
                    CStr::from_ptr(code).to_str().unwrap_or("")
                };
                (code, p)
            })
            .collect()
    };
    let stop = match decide(hooks.selected, &detected, no_speech, hooks.one_window) {
        Decision::Speak(index) => return i32::try_from(index).unwrap_or(-1),
        Decision::Other(index) => detected
            .get(index)
            .map(|&(code, _)| Stop::Other(code.to_owned())),
        Decision::Silent => Some(Stop::Silent),
        Decision::Unknown => Some(Stop::Unknown),
    };
    hooks.stop = stop;
    -1
}

extern "C" fn on_progress(context: *mut c_void, permille: i32) {
    let permille = u16::try_from(permille.clamp(0, 1000)).unwrap_or(0);
    // Caller waits for this thread: never gone.
    hooks(context)
        .events
        .send(Event::Progress(permille))
        .unwrap_or_default();
}

extern "C" fn on_segment(context: *mut c_void, text: *const c_char) {
    if text.is_null() {
        return;
    }
    // SAFETY: C passes NUL-terminated text, valid during call.
    #[allow(unsafe_code)]
    let text = unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned();
    hooks(context)
        .events
        .send(Event::Segment(text))
        .unwrap_or_default();
}

extern "C" fn on_cancelled(context: *mut c_void) -> i32 {
    i32::from(hooks(context).cancel.load(Ordering::Relaxed))
}

/// Engine lib's file in `libs`: 8.2 build where CPU has dotprod and fp16, else 8.0.
#[cfg(target_arch = "aarch64")]
fn library_name() -> &'static str {
    if std::arch::is_aarch64_feature_detected!("dotprod")
        && std::arch::is_aarch64_feature_detected!("fp16")
    {
        "libpiirit-speech-v82.so"
    } else {
        "libpiirit-speech-v8.so"
    }
}

/// Engine lib's file in `libs`: one build off aarch64.
#[cfg(not(target_arch = "aarch64"))]
fn library_name() -> &'static str {
    "libpiirit-speech.so"
}

/// Engine lib, open; closed on drop.
struct Library {
    handle: NonNull<c_void>,
    load: ffi::Load,
    free: ffi::Free,
    run: ffi::Run,
}

impl Library {
    fn open(path: &Path) -> Result<Self, Error> {
        let name = c_path(path).map_err(Error::Unavailable)?;
        // SAFETY: NUL-terminated path, outlives call; lib's initialisers are whisper.cpp's
        // and ggml's statics, safe to run.
        #[allow(unsafe_code)]
        let handle = unsafe { ffi::dlopen(name.as_ptr(), ffi::RTLD_NOW | ffi::RTLD_LOCAL) };
        let Some(handle) = NonNull::new(handle) else {
            return Err(Error::Unavailable(format!(
                "{}: {}",
                path.display(),
                dl_error()
            )));
        };
        let symbol = |name: &[u8]| {
            // SAFETY: open handle, NUL-terminated name.
            #[allow(unsafe_code)]
            let address = unsafe { ffi::dlsym(handle.as_ptr(), name.as_ptr().cast::<c_char>()) };
            NonNull::new(address).ok_or_else(|| {
                Error::Unavailable(format!(
                    "{}: no {}",
                    path.display(),
                    String::from_utf8_lossy(name).trim_end_matches('\0')
                ))
            })
        };
        let symbols = (|| {
            Ok::<_, Error>((
                symbol(b"piirit_speech_load\0")?,
                symbol(b"piirit_speech_free\0")?,
                symbol(b"piirit_speech_run\0")?,
            ))
        })();
        let (load, free, run) = match symbols {
            Ok(symbols) => symbols,
            Err(err) => {
                // SAFETY: open handle, closed once.
                #[allow(unsafe_code)]
                unsafe {
                    ffi::dlclose(handle.as_ptr());
                }
                return Err(err);
            }
        };
        // SAFETY: speech.h's three functions, these exact signatures (`ffi` mirrors it).
        #[allow(unsafe_code)]
        let library = unsafe {
            Self {
                handle,
                load: std::mem::transmute::<*mut c_void, ffi::Load>(load.as_ptr()),
                free: std::mem::transmute::<*mut c_void, ffi::Free>(free.as_ptr()),
                run: std::mem::transmute::<*mut c_void, ffi::Run>(run.as_ptr()),
            }
        };
        Ok(library)
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: open handle, closed once; nothing from it outlives `Engine`.
        #[allow(unsafe_code)]
        unsafe {
            ffi::dlclose(self.handle.as_ptr());
        }
    }
}

/// `dlerror()` text.
fn dl_error() -> String {
    // SAFETY: returns NULL or NUL-terminated text, valid until next dl call on this thread.
    #[allow(unsafe_code)]
    let text = unsafe { ffi::dlerror() };
    if text.is_null() {
        "could not load".into()
    } else {
        // SAFETY: as above.
        #[allow(unsafe_code)]
        let text = unsafe { CStr::from_ptr(text) };
        text.to_string_lossy().into_owned()
    }
}

/// Error buffer size for C messages.
const ERROR_LEN: usize = 512;
/// Whisper codes are 2-3 letters.
const LANGUAGE_LEN: usize = 16;

fn c_path(path: &Path) -> Result<CString, String> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes()).map_err(|err| {
        format!(
            "{} has a NUL in it, at byte {}",
            path.display(),
            err.nul_position()
        )
    })
}

/// Text C wrote into `buffer`, NUL-terminated or zeroed.
fn message(buffer: &[c_char]) -> String {
    let bytes: Vec<u8> = buffer
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| u8::from_ne_bytes(c.to_ne_bytes()))
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// C side, as `csrc/speech.h` declares it, plus libdl.
mod ffi {
    use std::ffi::{c_char, c_int, c_void};

    pub const OK: c_int = 0;
    pub const CANCELLED: c_int = 1;
    pub const STOPPED: c_int = 2;

    pub const RTLD_NOW: c_int = 2;
    pub const RTLD_LOCAL: c_int = 0;

    #[repr(C)]
    pub struct Hooks {
        pub context: *mut c_void,
        pub choose: extern "C" fn(*mut c_void, *const *const c_char, *const f32, i32, f32) -> i32,
        pub progress: extern "C" fn(*mut c_void, i32),
        pub segment: extern "C" fn(*mut c_void, *const c_char),
        pub cancelled: extern "C" fn(*mut c_void) -> i32,
    }

    #[repr(C)]
    pub struct Job {
        pub samples: *const f32,
        pub n_samples: i32,
        pub cpus: *const i32,
        pub n_cpus: i32,
        pub threads: i32,
        pub background: i32,
        pub audio_ctx: i32,
    }

    pub type Load = unsafe extern "C" fn(*const c_char, *mut c_char, usize) -> *mut c_void;
    pub type Free = unsafe extern "C" fn(*mut c_void);
    pub type Run = unsafe extern "C" fn(
        *mut c_void,
        *const Job,
        *const Hooks,
        *mut c_char,
        usize,
        *mut c_char,
        usize,
    ) -> c_int;

    // Harbour-allowed libdl; glibc 2.34+ has these in libc too.
    #[allow(unsafe_code)]
    #[link(name = "dl")]
    extern "C" {
        pub fn dlopen(file: *const c_char, mode: c_int) -> *mut c_void;
        pub fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        pub fn dlclose(handle: *mut c_void) -> c_int;
        pub fn dlerror() -> *mut c_char;
    }
}

#[cfg(test)]
mod tests {
    use super::{audio_context, big_cores_in, choose_language, decide, thread_count, Decision};

    fn codes(list: &[&str]) -> Vec<String> {
        list.iter().map(|&code| code.to_owned()).collect()
    }

    #[test]
    fn the_likeliest_selected_language_is_spoken() {
        let detected = [("en", 0.5), ("fi", 0.3), ("de", 0.2)];
        assert_eq!(
            choose_language(&codes(&["fi", "de"]), &detected),
            Decision::Speak(1)
        );
        // Top one selected: it.
        assert_eq!(
            choose_language(&codes(&["de", "en"]), &detected),
            Decision::Speak(0)
        );
    }

    #[test]
    fn a_language_over_twice_as_likely_as_any_selected_one_is_reported_instead() {
        let detected = [("en", 0.61), ("fi", 0.3), ("de", 0.09)];
        assert_eq!(
            choose_language(&codes(&["fi", "de"]), &detected),
            Decision::Other(0)
        );
        // Exactly twice: still selected one.
        let detected = [("en", 0.6), ("fi", 0.3), ("de", 0.1)];
        assert_eq!(
            choose_language(&codes(&["fi", "de"]), &detected),
            Decision::Speak(1)
        );
    }

    #[test]
    fn with_no_selected_language_known_the_likeliest_is_reported() {
        let detected = [("en", 0.2), ("fi", 0.7), ("de", 0.1)];
        assert_eq!(
            choose_language(&codes(&["xx", "yy"]), &detected),
            Decision::Other(1)
        );
        assert_eq!(choose_language(&codes(&["fi"]), &[]), Decision::Unknown);
    }

    #[test]
    fn unreadable_probabilities_count_as_unlikely() {
        let detected = [("en", f32::NAN), ("fi", 0.1), ("de", 0.05)];
        assert_eq!(
            choose_language(&codes(&["fi", "de"]), &detected),
            Decision::Speak(1)
        );
        let detected = [("en", 0.9), ("fi", f32::NAN), ("de", f32::NAN)];
        assert_eq!(
            choose_language(&codes(&["fi", "de"]), &detected),
            Decision::Other(0)
        );
    }

    #[test]
    fn a_silent_first_window_is_no_speech_whatever_the_languages() {
        let detected = [("en", 0.5), ("fi", 0.3), ("de", 0.2)];
        for selected in [
            codes(&[]),
            codes(&["fi"]),
            codes(&["fi", "de"]),
            codes(&["xx"]),
        ] {
            assert_eq!(decide(&selected, &detected, 0.61, true), Decision::Silent);
            // At whisper's threshold: still speech.
            assert_ne!(decide(&selected, &detected, 0.6, true), Decision::Silent);
        }
        // Longer clip: speech may come after first window.
        assert_eq!(
            decide(&codes(&[]), &detected, 0.99, false),
            Decision::Speak(0)
        );
        assert_eq!(
            decide(&codes(&[]), &detected, f32::NAN, true),
            Decision::Speak(0)
        );
    }

    #[test]
    fn any_language_is_the_likeliest_and_one_given_follows_the_rule_for_several() {
        let detected = [("en", 0.2), ("fi", 0.7), ("de", 0.1)];
        assert_eq!(
            decide(&codes(&[]), &detected, 0.0, true),
            Decision::Speak(1)
        );
        // Regression (device run 41883450707): only English picked, German clip came out
        // as English. Unselected language over twice as likely: reported.
        assert_eq!(
            decide(&codes(&["de"]), &detected, 0.0, true),
            Decision::Other(1)
        );
        assert_eq!(
            decide(&codes(&["en"]), &[("en", 0.4), ("de", 0.6)], 0.0, true),
            Decision::Speak(0)
        );
        // Several: rule above.
        assert_eq!(
            decide(&codes(&["en", "de"]), &detected, 0.0, true),
            Decision::Other(1)
        );
        // Equals: first.
        let even = [("sv", 0.4), ("fi", 0.4), ("de", 0.2)];
        assert_eq!(decide(&codes(&[]), &even, 0.0, true), Decision::Speak(0));
    }

    #[test]
    fn a_short_clip_is_encoded_short_and_a_long_one_whole() {
        // 5 s: 250 positions, plus margin, in 64s.
        assert_eq!(audio_context(5 * 16_000), 384);
        assert_eq!(audio_context(10 * 16_000), 640);
        // Never past one window.
        assert_eq!(audio_context(29 * 16_000), 1500);
        assert_eq!(audio_context(30 * 16_000), 1500);
        // Past one window: model's own, as whisper.cpp moves 30 s per step.
        assert_eq!(audio_context(30 * 16_000 + 320), 0);
        assert_eq!(audio_context(120 * 16_000), 0);
        assert_eq!(audio_context(1), 192);
    }

    #[test]
    fn threads_follow_the_cores_given_then_the_big_ones() {
        assert_eq!(thread_count(&[4, 5], || vec![0, 1, 2]), 2);
        assert_eq!(thread_count(&[], || vec![4, 5, 6, 7]), 4);
        assert_eq!(thread_count(&[], || vec![6, 7]), 2);
        assert_eq!(thread_count(&[], Vec::new), 4);
        assert_eq!(thread_count(&[], || (0..64).collect()), 8);
    }

    /// Fake sysfs cpu dir: `(name, max freq)`; `None` = no cpufreq.
    fn fake_sysfs(name: &str, cores: &[(&str, Option<&str>)]) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("piirit-speech-sysfs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (core, freq) in cores {
            let dir = root.join(core);
            std::fs::create_dir_all(&dir).unwrap();
            if let Some(freq) = freq {
                std::fs::create_dir_all(dir.join("cpufreq")).unwrap();
                std::fs::write(dir.join("cpufreq/cpuinfo_max_freq"), format!("{freq}\n")).unwrap();
            }
        }
        root
    }

    #[test]
    fn big_cores_are_the_fastest_ones() {
        // Snapdragon 845 shape: 4 x 1.77 GHz, 4 x 2.8 GHz; plus sysfs's other entries.
        let root = fake_sysfs(
            "big-little",
            &[
                ("cpu0", Some("1766400")),
                ("cpu1", Some("1766400")),
                ("cpu2", Some("1766400")),
                ("cpu3", Some("1766400")),
                ("cpu4", Some("2803200")),
                ("cpu5", Some("2803200")),
                ("cpu6", Some("2803200")),
                ("cpu7", Some("2803200")),
                ("cpufreq", None),
                ("cpuidle", None),
            ],
        );
        assert_eq!(big_cores_in(&root), vec![4, 5, 6, 7]);
        std::fs::remove_dir_all(&root).unwrap();

        // Dimensity 7100 shape: 4 + 4, prime-less; and one core unreadable (offline).
        let root = fake_sysfs(
            "unreadable",
            &[
                ("cpu0", Some("2000000")),
                ("cpu1", Some("2000000")),
                ("cpu6", Some("2600000")),
                ("cpu7", None),
                ("cpu10", Some("2600000")),
            ],
        );
        assert_eq!(big_cores_in(&root), vec![6, 10]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn equal_cores_are_all_big_and_unknown_ones_none() {
        let root = fake_sysfs(
            "equal",
            &[("cpu1", Some("1800000")), ("cpu0", Some("1800000"))],
        );
        assert_eq!(big_cores_in(&root), vec![0, 1]);
        std::fs::remove_dir_all(&root).unwrap();

        let root = fake_sysfs("unknown", &[("cpu0", None), ("cpu1", None)]);
        assert!(big_cores_in(&root).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
        assert!(big_cores_in(&root).is_empty());
    }
}
