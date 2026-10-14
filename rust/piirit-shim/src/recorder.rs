//! Voice message recorder, platform audio stack.
//!
//! Qt 5.6 QML has no recorder (`QAudioRecorder` is C++ only, unbound by qmetaobject): hence
//! tree's second `cpp!` block after `piirit-app/src/translations.rs`, and shim's C++ build step.
//! Kept to this module, short blocks.
//!
//! WAV via `GStreamer`, encoded to MP3 as it lands (`voice.rs` says why MP3). WAV is scratch in
//! captures dir (`capture.rs`). MP3 sent as core `Voice` viewtype, so drawn as voice, not music.
//!
//! Stops itself at relay limit: page passes core's attachment limit, `limit_ms` follows, stop as
//! a send tap would. Page polls on a timer while recording, no signal bridge; each poll encodes
//! new data. `GStreamer` stop is async (file finished after `stop()`), so reported once poll sees
//! recorder no longer finalising.

// `cpp!` is unsafe FFI. Allowed here only; docs/BUILDING.md says why.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::path::PathBuf;

use cpp::cpp;
use qmetaobject::*;

use crate::voice::{self, Transcoder};

cpp! {{
    #include <QtCore/QCoreApplication>
    #include <QtCore/QString>
    #include <QtCore/QStringList>
    #include <QtCore/QUrl>
    #include <QtMultimedia/QAudioEncoderSettings>
    #include <QtMultimedia/QAudioRecorder>
    #include <QtMultimedia/QMediaRecorder>
}}

/// Codec and container to record in.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Format {
    /// Plain samples.
    codec: String,
    /// WAV.
    container: String,
}

/// Pick plain-sample WAV from what recorder offers.
///
/// Names vary: Sailfish plugin says `audio/PCM`, `wav`; `GStreamer` names `audio/x-raw`,
/// `audio/x-wav`. Lowercase substring match.
/// No fallback: recorder's encoders made voice FLAC; Ogg is a file on iOS.
/// None offered (headless, no `GStreamer`): `None`, no mic button.
fn choose_format(codecs: &[String], containers: &[String]) -> Option<Format> {
    let named = |name: &&String, words: &[&str]| {
        let name = name.to_ascii_lowercase();
        words.iter().any(|word| name.contains(word))
    };
    let codec = codecs.iter().find(|name| named(name, &["pcm", "x-raw"]))?;
    let container = containers.iter().find(|name| named(name, &["wav"]))?;
    Some(Format {
        codec: codec.clone(),
        container: container.clone(),
    })
}

/// `QMediaRecorder::Status`, as C++ reports.
const FINALIZING_STATUS: i32 = 7;
/// `QMediaRecorder::State::RecordingState`.
const RECORDING_STATE: i32 = 1;

/// Records voice message.
///
/// ```qml
/// VoiceRecorder { id: recorder; limit_bytes: messages.attachment_limit
///                 onRecorded: messages.send_voice(path) }
/// IconButton { visible: recorder.available; onClicked: recorder.start(path) }
/// Timer { running: recorder.recording; onTriggered: recorder.poll() }
/// ```
#[derive(QObject, Default)]
// `available`, `recording` bind separately in QML: no state machine.
#[allow(clippy::struct_excessive_bools)]
pub struct VoiceRecorder {
    base: qt_base_class!(trait QObject),

    /// Audio input and encoder present. False headless or without encoders: no mic button.
    pub available: qt_property!(bool; READ is_available),
    /// Extension: `mp3`, or empty when nothing recordable.
    pub extension: qt_property!(QString; READ extension_name),

    /// Relay limit in bytes, from core attachment limit; 0 unknown (no limit but reader's).
    /// `f64`: no 64-bit in QML.
    pub limit_bytes: qt_property!(f64; WRITE set_limit_bytes NOTIFY limit_changed),
    /// Core's `media_quality`: 0 balanced, 1 less data. MP3 bit rate follows it.
    pub media_quality: qt_property!(u32; WRITE set_media_quality NOTIFY limit_changed),
    /// Longest recording in ms whose MP3 fits `limit_bytes`; 0 no limit. While recording: that
    /// recording's.
    pub limit_ms: qt_property!(u32; READ longest NOTIFY limit_changed),
    /// Emitted when [`Self::limit_ms`] may change.
    pub limit_changed: qt_signal!(),

    /// True from `start` until reported or dropped.
    pub recording: qt_property!(bool; NOTIFY recording_changed),
    /// Emitted when [`Self::recording`] changes.
    pub recording_changed: qt_signal!(),
    /// Ms recorded as of last `poll`.
    pub duration_ms: qt_property!(u32; NOTIFY duration_changed),
    /// Emitted on [`Self::duration_ms`] change.
    pub duration_changed: qt_signal!(),

    /// Record into `path` (MP3). Answers `recorded` once stopped (or limit hit) and file
    /// finished; or `error`.
    pub start: qt_method!(fn(&mut self, path: QString)),
    /// Stop; report once finished.
    pub stop: qt_method!(fn(&mut self)),
    /// Stop; discard file.
    pub cancel: qt_method!(fn(&mut self)),
    /// Encode new data, stop at limit, finish pending stop. Page calls on timer while recording.
    pub poll: qt_method!(fn(&mut self)),

    /// Recording finished, at `path`.
    pub recorded: qt_signal!(path: QString),
    /// Failed to start, or on the way.
    pub error: qt_signal!(message: QString),

    /// Made on first use (needs app object).
    handle: usize,
    /// MP3 destination.
    path: String,
    /// WAV being encoded, while recording.
    transcoder: Option<Transcoder>,
    /// `stop` called; file still finishing.
    finishing: bool,
}

/// Platform recorder capabilities, once per process.
///
/// Probing is slow: loads `GStreamer` capture service, lists devices, element factories, on Qt
/// thread, as each chat page builds. Never changes, so first recorder answers for all; others
/// make a `QAudioRecorder` only to record.
#[derive(Debug)]
struct Probe {
    /// Format to record in; `None` if nothing recordable.
    chosen: Option<Format>,
    /// Input to record from; `None` leaves recorder's own.
    input: Option<String>,
}

/// The process's [`Probe`], once a recorder has made it.
static PROBE: std::sync::OnceLock<Probe> = std::sync::OnceLock::new();

impl VoiceRecorder {
    /// Anything recordable.
    pub fn is_available(&mut self) -> bool {
        self.chosen().is_some()
    }

    /// Recording extension.
    pub fn extension_name(&mut self) -> QString {
        if self.chosen().is_some() {
            QString::from("mp3")
        } else {
            QString::default()
        }
    }

    /// Take relay limit; hold recording under way to it too.
    pub fn set_limit_bytes(&mut self, bytes: f64) {
        if self.limit_bytes.to_bits() == bytes.to_bits() {
            return;
        }
        self.limit_bytes = bytes;
        let limit = self.limit();
        if let Some(transcoder) = self.transcoder.as_mut() {
            transcoder.set_limit_bytes(limit);
        }
        self.limit_changed();
    }

    /// Take media quality, for next recording.
    pub fn set_media_quality(&mut self, quality: u32) {
        if self.media_quality == quality {
            return;
        }
        self.media_quality = quality;
        self.limit_changed();
    }

    /// Limit in bytes; 0 while core has not said.
    fn limit(&self) -> u64 {
        // Exact to 2^53 bytes.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        {
            self.limit_bytes.max(0.0) as u64
        }
    }

    /// Longest recording in ms; 0 for no limit.
    #[must_use]
    pub fn longest(&self) -> u32 {
        match &self.transcoder {
            Some(transcoder) => transcoder.limit_ms(),
            None => voice::longest_ms(self.limit(), voice::bit_rate(self.media_quality)),
        }
    }

    /// Format to record in; `None` if nothing can be ([`Probe`]).
    /// Not kept without app object: a later recorder asks again.
    fn chosen(&mut self) -> Option<Format> {
        if let Some(probe) = PROBE.get() {
            return probe.chosen.clone();
        }
        let handle = self.recorder();
        if handle == 0 {
            return None;
        }
        let probe = PROBE.get_or_init(|| read_probe(handle));
        // Made before an answer, so input was unset: set now.
        if let Some(input) = &probe.input {
            set_input(handle, input);
        }
        probe.chosen.clone()
    }

    /// `QAudioRecorder` on first use, with process's input. 0 without app object.
    fn recorder(&mut self) -> usize {
        if self.handle == 0 {
            // SAFETY: owned by this object, deleted in `Drop`. Qt thread only.
            let handle = cpp!(unsafe [] -> *mut c_void as "void*" {
                if (!QCoreApplication::instance()) {
                    return nullptr;
                }
                return new QAudioRecorder();
            });
            self.handle = handle as usize;
            if self.handle != 0 {
                if let Some(input) = PROBE.get().and_then(|probe| probe.input.as_deref()) {
                    set_input(self.handle, input);
                }
            }
        }
        self.handle
    }

    /// Start recording into `path`.
    pub fn start(&mut self, path: QString) {
        if self.recording {
            return;
        }
        let Some(format) = self.chosen() else {
            self.error(QString::from("nothing here can record sound"));
            return;
        };
        let path = path.to_string();
        if path.is_empty() {
            self.error(QString::from("nowhere to record to"));
            return;
        }
        let handle = self.recorder();
        if handle == 0 {
            self.error(QString::from("nothing here can record sound"));
            return;
        }
        // Beside MP3, own name.
        let wav = format!("{path}.wav");
        let transcoder = match Transcoder::new(
            PathBuf::from(&wav),
            PathBuf::from(&path),
            voice::bit_rate(self.media_quality),
            self.limit(),
        ) {
            Ok(transcoder) => transcoder,
            Err(message) => {
                self.error(message.into());
                return;
            }
        };
        let codec = QString::from(format.codec);
        let container = QString::from(format.container);
        let sample_rate = i32::try_from(voice::SAMPLE_RATE).unwrap_or(0);
        let location = QString::from(wav);
        let recorder = handle as *mut c_void;
        // SAFETY: `recorder` owned here; QStrings owned by frame.
        let started = cpp!(unsafe [recorder as "QAudioRecorder*", codec as "QString",
                                   container as "QString", sample_rate as "int",
                                   location as "QString"] -> bool as "bool" {
            QAudioEncoderSettings settings;
            settings.setCodec(codec);
            // Mono: half the bytes.
            settings.setChannelCount(1);
            if (sample_rate > 0) {
                settings.setSampleRate(sample_rate);
            }
            settings.setEncodingMode(QMultimedia::ConstantQualityEncoding);
            settings.setQuality(QMultimedia::NormalQuality);
            recorder->setEncodingSettings(settings, QVideoEncoderSettings(), container);
            recorder->setOutputLocation(QUrl::fromLocalFile(location));
            recorder->record();
            return recorder->error() == QMediaRecorder::NoError;
        });
        if !started {
            transcoder.discard();
            self.error(QString::from(read_error(handle)));
            return;
        }
        self.path = path;
        self.transcoder = Some(transcoder);
        self.finishing = false;
        self.duration_ms = 0;
        self.duration_changed();
        self.recording = true;
        self.recording_changed();
        self.limit_changed();
    }

    /// Stop; report once finished.
    pub fn stop(&mut self) {
        if !self.recording || self.finishing {
            return;
        }
        let handle = self.recorder();
        stop_recording(handle);
        self.finishing = true;
        // Backend may be done already: poll now.
        self.poll();
    }

    /// Stop; discard.
    pub fn cancel(&mut self) {
        if !self.recording {
            return;
        }
        let handle = self.recorder();
        stop_recording(handle);
        self.drop_recording(handle);
    }

    /// Re-read duration; encode; stop at limit.
    pub fn poll(&mut self) {
        if !self.recording {
            return;
        }
        let handle = self.recorder();
        let (duration, status, state, error) = read_progress(handle);
        if duration != self.duration_ms {
            self.duration_ms = duration;
            self.duration_changed();
        }
        if !error.is_empty() {
            self.drop_recording(handle);
            self.error(error.into());
            return;
        }
        if self.finishing {
            if state != RECORDING_STATE && status != FINALIZING_STATUS {
                self.finish(handle);
            }
            return;
        }
        if let Err(message) = self.transcoder.as_mut().map_or(Ok(()), Transcoder::pump) {
            stop_recording(handle);
            self.drop_recording(handle);
            self.error(message.into());
            return;
        }
        // At limit: stop and send, as tap on send. MP3 keeps to limit whatever recorder writes
        // after.
        let limit = self.longest();
        if limit > 0 && duration >= limit {
            self.stop();
        }
    }

    /// Recorder done: finish MP3, report.
    fn finish(&mut self, handle: usize) {
        let path = std::mem::take(&mut self.path);
        let result = match self.transcoder.take() {
            Some(mut transcoder) => {
                // Backend's own WAV path, if it chose one.
                let actual = read_actual_location(handle);
                if !actual.is_empty() {
                    transcoder.follow(PathBuf::from(actual));
                }
                transcoder.finish()
            }
            None => Ok(None),
        };
        self.finishing = false;
        self.recording = false;
        self.recording_changed();
        self.limit_changed();
        match result {
            Ok(Some(_)) => self.recorded(path.into()),
            Ok(None) => self.error(QString::from("nothing was recorded")),
            Err(message) => self.error(message.into()),
        }
    }

    /// Forget recording; remove what it wrote.
    fn drop_recording(&mut self, handle: usize) {
        if let Some(transcoder) = self.transcoder.take() {
            transcoder.discard();
        }
        let actual = read_actual_location(handle);
        if !actual.is_empty() {
            if let Err(err) = std::fs::remove_file(&actual) {
                eprintln!("piirit: removing abandoned recording {actual}: {err}");
            }
        }
        self.path.clear();
        self.finishing = false;
        self.recording = false;
        self.recording_changed();
        self.limit_changed();
    }
}

impl Drop for VoiceRecorder {
    fn drop(&mut self) {
        // Page closed mid-recording: discard.
        if let Some(transcoder) = self.transcoder.take() {
            transcoder.discard();
        }
        if self.handle == 0 {
            return;
        }
        let recorder = self.handle as *mut c_void;
        // SAFETY: made by `recorder()`, deleted once here.
        cpp!(unsafe [recorder as "QAudioRecorder*"] {
            recorder->stop();
            delete recorder;
        });
        self.handle = 0;
    }
}

/// Ask recorder what it writes and reads, then choose.
fn read_probe(handle: usize) -> Probe {
    let codecs = read_list(handle, ListKind::Codecs);
    let containers = read_list(handle, ListKind::Containers);
    let inputs = read_list(handle, ListKind::Inputs);
    let default = read_default_input(handle);
    Probe {
        chosen: choose_format(&codecs, &containers),
        input: choose_input(&inputs, &default),
    }
}

/// Recorder lists: codecs, containers, inputs.
#[derive(Clone, Copy)]
enum ListKind {
    Codecs,
    Containers,
    Inputs,
}

/// One list by name.
fn read_list(handle: usize, kind: ListKind) -> Vec<String> {
    let recorder = handle as *mut c_void;
    let kind = kind as i32;
    // SAFETY: live QAudioRecorder; answer joined into owned QString.
    let joined = cpp!(unsafe [recorder as "QAudioRecorder*", kind as "int"] -> QString as "QString" {
        QStringList names;
        switch (kind) {
        case 0: names = recorder->supportedAudioCodecs(); break;
        case 1: names = recorder->supportedContainers(); break;
        default: names = recorder->audioInputs(); break;
        }
        return names.join(QLatin1Char('\n'));
    });
    joined
        .to_string()
        .lines()
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Input recorder uses by itself.
fn read_default_input(handle: usize) -> String {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    let name = cpp!(unsafe [recorder as "QAudioRecorder*"] -> QString as "QString" {
        return recorder->defaultAudioInput();
    });
    name.to_string()
}

/// Tell recorder which input to record from.
fn set_input(handle: usize, input: &str) {
    let recorder = handle as *mut c_void;
    let input = QString::from(input);
    // SAFETY: as `read_list`; QString owned by frame.
    cpp!(unsafe [recorder as "QAudioRecorder*", input as "QString"] {
        recorder->setAudioInput(input);
    });
}

/// Pick input from what recorder offers.
///
/// `PulseAudio` by name if offered: backend default picked a silent source on phone.
/// Else backend default, else first listed.
/// None offered: recorder left alone.
fn choose_input(inputs: &[String], default: &str) -> Option<String> {
    inputs
        .iter()
        .find(|name| name.to_ascii_lowercase().contains("pulse"))
        .or_else(|| inputs.iter().find(|name| name.as_str() == default))
        .or_else(|| inputs.first())
        .cloned()
}

/// Ask recorder to stop; file finishes a moment later.
fn stop_recording(handle: usize) {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    cpp!(unsafe [recorder as "QAudioRecorder*"] {
        recorder->stop();
    });
}

/// Ms recorded, status, state, error if any.
fn read_progress(handle: usize) -> (u32, i32, i32, String) {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    let duration = cpp!(unsafe [recorder as "QAudioRecorder*"] -> i64 as "qint64" {
        return recorder->duration();
    });
    let status = cpp!(unsafe [recorder as "QAudioRecorder*"] -> i32 as "int" {
        return static_cast<int>(recorder->status());
    });
    let state = cpp!(unsafe [recorder as "QAudioRecorder*"] -> i32 as "int" {
        return static_cast<int>(recorder->state());
    });
    let duration = u32::try_from(duration.max(0)).unwrap_or(u32::MAX);
    (duration, status, state, read_error(handle))
}

/// Recorder error message; empty if none.
fn read_error(handle: usize) -> String {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    let message = cpp!(unsafe [recorder as "QAudioRecorder*"] -> QString as "QString" {
        if (recorder->error() == QMediaRecorder::NoError) {
            return QString();
        }
        QString text = recorder->errorString();
        return text.isEmpty() ? QStringLiteral("recording failed") : text;
    });
    message.to_string()
}

/// Where recorder wrote: local path or empty.
fn read_actual_location(handle: usize) -> String {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    let location = cpp!(unsafe [recorder as "QAudioRecorder*"] -> QString as "QString" {
        return recorder->actualLocation().toLocalFile();
    });
    location.to_string()
}

#[cfg(test)]
mod tests {
    use super::{choose_format, choose_input, Format};

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    fn format(codec: &str, container: &str) -> Format {
        Format {
            codec: codec.into(),
            container: container.into(),
        }
    }

    #[test]
    fn plain_samples_in_wav_are_what_is_recorded() {
        // Sailfish plugin: own names, no AAC.
        let codecs = names(&["audio/vorbis", "audio/PCM", "audio/FLAC", "audio/opus"]);
        let containers = names(&["matroska", "ogg", "wav", "raw"]);
        assert_eq!(
            choose_format(&codecs, &containers),
            Some(format("audio/PCM", "wav"))
        );
        // MP3 offered too: still plain samples.
        let with_mp3 = names(&["audio/mpeg", "audio/PCM", "audio/FLAC"]);
        assert_eq!(
            choose_format(&with_mp3, &containers),
            Some(format("audio/PCM", "wav"))
        );
        // GStreamer's own names.
        let gst = names(&["audio/x-flac", "audio/x-raw", "audio/x-opus"]);
        let gst_containers = names(&["application/ogg", "audio/x-wav"]);
        assert_eq!(
            choose_format(&gst, &gst_containers),
            Some(format("audio/x-raw", "audio/x-wav"))
        );
        // Encoders, no plain samples: nothing, not a file iOS shows.
        let encoders_only = names(&["audio/FLAC", "audio/opus", "audio/vorbis"]);
        assert_eq!(choose_format(&encoders_only, &containers), None);
        assert_eq!(choose_format(&codecs, &names(&["ogg", "raw"])), None);
        // Nothing offered: nothing chosen.
        assert_eq!(choose_format(&[], &containers), None);
        assert_eq!(choose_format(&codecs, &[]), None);
    }

    #[test]
    fn the_sound_server_is_the_input_where_it_is_offered() {
        // GStreamer on device: auto source, sound server, ALSA.
        let inputs = names(&["default:", "pulseaudio:", "alsa:hw:0,0"]);
        assert_eq!(
            choose_input(&inputs, "default:"),
            Some("pulseaudio:".to_string())
        );
        // No sound server: backend default, if listed.
        let without = names(&["alsa:hw:0,0", "default:"]);
        assert_eq!(
            choose_input(&without, "default:"),
            Some("default:".to_string())
        );
        // Default not listed: first thing.
        assert_eq!(
            choose_input(&without, "oss:"),
            Some("alsa:hw:0,0".to_string())
        );
        // Nothing offered: nothing chosen.
        assert_eq!(choose_input(&[], "default:"), None);
    }
}
