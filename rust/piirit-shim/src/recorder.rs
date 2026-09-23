//! A voice message, recorded through the platform's own audio stack.
//!
//! QML on Qt 5.6 has no audio recorder: `QAudioRecorder` is C++ only, and
//! qmetaobject does not bind it. So this is the tree's second `cpp!`
//! block, after the translator (`piirit-app/src/translations.rs`), and
//! the reason the shim has a C++ build step: the recorder is made, driven
//! and asked about from the few lines of C++ below, and everything else
//! is Rust. The exception is kept to this module, and to blocks short
//! enough to be checked by reading.
//!
//! What is recorded goes through `GStreamer` on a device, as plain
//! samples in WAV, and is encoded to MP3 as it lands (`voice.rs`): the
//! recorder offers nothing on the phone that every other client plays as
//! a voice message, and that module says why MP3 is. The WAV is scratch
//! beside the MP3 in the captures directory (`capture.rs`); the MP3 is
//! sent as a voice message -- the core's `Voice` view type, which is what
//! draws it as one at the other end rather than as a music file.
//!
//! A recording stops itself at the longest the relay takes. The page
//! hands over the core's attachment limit, the recorder says how long a
//! recording that fits is (`limit_ms`), and it stops there the way a tap
//! on send stops it, so that what was said goes out rather than a file
//! the relay refuses.
//!
//! Nothing is connected to the recorder's signals: the page polls, on a
//! timer it runs only while recording, which is one call rather than a
//! signal bridge, and each poll encodes what was written since the last.
//! Stopping is asynchronous in the `GStreamer` backend -- the file is
//! finished a moment after `stop()` returns -- so the recording is
//! reported once the recorder says it is no longer finalising, from that
//! same poll.

// `cpp!` expands to a call across the FFI boundary, which is `unsafe` by
// construction. Scoped to this file: the workspace denies it everywhere
// else, and docs/BUILDING.md says why this one is allowed.
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

/// The codec and container to record in, as the recorder names them.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Format {
    /// Plain samples.
    codec: String,
    /// WAV.
    container: String,
}

/// Pick plain samples in WAV out of what the recorder offers.
///
/// What the recorder calls them varies: Qt's capture plugin on Sailfish
/// names them `audio/PCM` and `wav`, and a build that reads `GStreamer`'s
/// own names says `audio/x-raw` and `audio/x-wav`. So a name is matched
/// on whether it contains the word, in lower case.
///
/// Nothing else is taken instead: the recorder's own encoders are what
/// made voice messages FLAC, and Ogg is a file rather than a voice
/// message on iOS. Neither offered -- the headless test runner, or a
/// phone with no `GStreamer` at all -- is `None`, which the page shows as
/// no microphone button.
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

/// `QMediaRecorder::Status`, as the C++ reports it.
const FINALIZING_STATUS: i32 = 7;
/// `QMediaRecorder::State::RecordingState`.
const RECORDING_STATE: i32 = 1;

/// Records a voice message.
///
/// ```qml
/// VoiceRecorder { id: recorder; limit_bytes: messages.attachment_limit
///                 onRecorded: messages.send_voice(path) }
/// IconButton { visible: recorder.available; onClicked: recorder.start(path) }
/// Timer { running: recorder.recording; onTriggered: recorder.poll() }
/// ```
#[derive(QObject, Default)]
// `available` and `recording` are two facts QML binds to on their own;
// `finishing` and `probed` are the recorder's own bookkeeping. They are
// not states of one thing, and a state machine over them would hide the
// two bindings behind it.
#[allow(clippy::struct_excessive_bools)]
pub struct VoiceRecorder {
    base: qt_base_class!(trait QObject),

    /// Whether anything can be recorded at all: an audio input and an
    /// encoder to write it with. False headlessly, and on a phone with
    /// no encoders, and then the page offers no microphone.
    pub available: qt_property!(bool; READ is_available),
    /// The extension a recording will have: `mp3`, or empty when nothing
    /// can be recorded.
    pub extension: qt_property!(QString; READ extension_name),

    /// The largest file the relay takes, in bytes, from the core's
    /// attachment limit; 0 while it is not known, and then a recording
    /// has no end but the reader's. A real because QML has no 64-bit
    /// integer.
    pub limit_bytes: qt_property!(f64; WRITE set_limit_bytes NOTIFY limit_changed),
    /// The reader's outgoing media quality, the core's `media_quality`: 0
    /// balanced, 1 less data. What the MP3's bit rate follows.
    pub media_quality: qt_property!(u32; WRITE set_media_quality NOTIFY limit_changed),
    /// The longest a recording can be, in milliseconds, for its MP3 to
    /// fit in [`Self::limit_bytes`]; 0 when there is no limit. While
    /// recording, that of the recording under way.
    pub limit_ms: qt_property!(u32; READ longest NOTIFY limit_changed),
    /// Emitted when [`Self::limit_ms`] may have changed.
    pub limit_changed: qt_signal!(),

    /// True from `start` until the recording is reported or dropped.
    pub recording: qt_property!(bool; NOTIFY recording_changed),
    /// Emitted when [`Self::recording`] changes.
    pub recording_changed: qt_signal!(),
    /// Milliseconds recorded so far, as of the last `poll`.
    pub duration_ms: qt_property!(u32; NOTIFY duration_changed),
    /// Emitted when [`Self::duration_ms`] changes.
    pub duration_changed: qt_signal!(),

    /// Start recording into `path`, an MP3. Answers on `recorded` once
    /// `stop` has been called, or the limit reached, and the file is
    /// finished; or on `error`.
    pub start: qt_method!(fn(&mut self, path: QString)),
    /// Stop, and report the file once it is finished.
    pub stop: qt_method!(fn(&mut self)),
    /// Stop, and throw the file away.
    pub cancel: qt_method!(fn(&mut self)),
    /// Encode what has been recorded since the last call, stop at the
    /// limit, and finish a stop that is under way. The page calls this on
    /// a timer while recording.
    pub poll: qt_method!(fn(&mut self)),

    /// The recording is finished and at `path`.
    pub recorded: qt_signal!(path: QString),
    /// Recording could not start, or failed on the way.
    pub error: qt_signal!(message: QString),

    /// The `QAudioRecorder`, made on first use: it needs the application
    /// object, which does not exist when QML builds this.
    handle: usize,
    /// Where the MP3 is going.
    path: String,
    /// The WAV being made into it, while recording.
    transcoder: Option<Transcoder>,
    /// `stop` has been called and the file is still being finished.
    finishing: bool,
    /// Cached from the recorder, so QML's bindings need not ask C++.
    chosen: Option<Format>,
    probed: bool,
}

impl VoiceRecorder {
    /// Whether anything can be recorded at all.
    pub fn is_available(&mut self) -> bool {
        self.probe();
        self.chosen.is_some()
    }

    /// The extension a recording will have.
    pub fn extension_name(&mut self) -> QString {
        self.probe();
        if self.chosen.is_some() {
            QString::from("mp3")
        } else {
            QString::default()
        }
    }

    /// Take the relay's limit, and hold a recording under way to it too.
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

    /// Take the reader's outgoing media quality, for the next recording.
    pub fn set_media_quality(&mut self, quality: u32) {
        if self.media_quality == quality {
            return;
        }
        self.media_quality = quality;
        self.limit_changed();
    }

    /// The limit as bytes; 0 while the core has not said.
    fn limit(&self) -> u64 {
        // Exact to 2^53 bytes, which no relay takes.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        {
            self.limit_bytes.max(0.0) as u64
        }
    }

    /// The longest a recording can be, in milliseconds; 0 for no limit.
    #[must_use]
    pub fn longest(&self) -> u32 {
        match &self.transcoder {
            Some(transcoder) => transcoder.limit_ms(),
            None => voice::longest_ms(self.limit(), voice::bit_rate(self.media_quality)),
        }
    }

    /// Make the recorder if it is not there yet, and find out what it
    /// can write.
    fn probe(&mut self) {
        if self.probed {
            return;
        }
        self.probed = true;
        let handle = self.recorder();
        if handle == 0 {
            return;
        }
        let codecs = read_list(handle, ListKind::Codecs);
        let containers = read_list(handle, ListKind::Containers);
        self.chosen = choose_format(&codecs, &containers);
        let inputs = read_list(handle, ListKind::Inputs);
        let default = read_default_input(handle);
        if let Some(input) = choose_input(&inputs, &default) {
            set_input(handle, &input);
        }
    }

    /// The `QAudioRecorder`, made on first use. 0 when there is no
    /// application object to hang it off, which is when nothing can be
    /// recorded either.
    fn recorder(&mut self) -> usize {
        if self.handle == 0 {
            // SAFETY: nothing is passed in, and the recorder made here is
            // owned by this object, which deletes it in `Drop`. It is
            // only ever used from the Qt thread, which is where every
            // method of this object runs.
            let handle = cpp!(unsafe [] -> *mut c_void as "void*" {
                if (!QCoreApplication::instance()) {
                    return nullptr;
                }
                return new QAudioRecorder();
            });
            self.handle = handle as usize;
        }
        self.handle
    }

    /// Start recording into `path`.
    pub fn start(&mut self, path: QString) {
        if self.recording {
            return;
        }
        self.probe();
        let Some(format) = self.chosen.clone() else {
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
        // Beside the MP3, under a name of its own whatever the MP3 is
        // called.
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
        // SAFETY: `recorder` is the QAudioRecorder this object made and
        // still owns; the three QStrings are owned by this frame and read
        // by value.
        let started = cpp!(unsafe [recorder as "QAudioRecorder*", codec as "QString",
                                   container as "QString", sample_rate as "int",
                                   location as "QString"] -> bool as "bool" {
            QAudioEncoderSettings settings;
            settings.setCodec(codec);
            // One channel: a voice, and half the bytes of two.
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

    /// Stop, and report the file once it is finished.
    pub fn stop(&mut self) {
        if !self.recording || self.finishing {
            return;
        }
        let handle = self.recorder();
        stop_recording(handle);
        self.finishing = true;
        // The backend may already be done; a poll now saves a tick.
        self.poll();
    }

    /// Stop, and throw the file away.
    pub fn cancel(&mut self) {
        if !self.recording {
            return;
        }
        let handle = self.recorder();
        stop_recording(handle);
        self.drop_recording(handle);
    }

    /// Re-read the duration, encode what has been written since the last
    /// poll, stop at the limit, and finish a stop that is under way.
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
        // At the longest the relay takes: stop, and send what there is,
        // as a tap on send would. The MP3 keeps to the limit whatever the
        // recorder writes after this.
        let limit = self.longest();
        if limit > 0 && duration >= limit {
            self.stop();
        }
    }

    /// The recorder is done: finish the MP3 and report it.
    fn finish(&mut self, handle: usize) {
        let path = std::mem::take(&mut self.path);
        let result = match self.transcoder.take() {
            Some(mut transcoder) => {
                // Where the backend put the WAV, which is where it was
                // asked to unless the backend had its own idea about it.
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

    /// Forget the recording under way, and remove what it wrote.
    fn drop_recording(&mut self, handle: usize) {
        if let Some(transcoder) = self.transcoder.take() {
            transcoder.discard();
        }
        let actual = read_actual_location(handle);
        if !actual.is_empty() {
            let _ = std::fs::remove_file(actual);
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
        // A recording the page was closed on is not going anywhere.
        if let Some(transcoder) = self.transcoder.take() {
            transcoder.discard();
        }
        if self.handle == 0 {
            return;
        }
        let recorder = self.handle as *mut c_void;
        // SAFETY: the recorder was made by `recorder()` and is deleted
        // exactly once, here; the handle is zeroed so nothing can reach
        // it afterwards.
        cpp!(unsafe [recorder as "QAudioRecorder*"] {
            recorder->stop();
            delete recorder;
        });
        self.handle = 0;
    }
}

/// The lists a recorder has: what it can write, and what it can read.
#[derive(Clone, Copy)]
enum ListKind {
    Codecs,
    Containers,
    Inputs,
}

/// One of the recorder's lists, by name.
fn read_list(handle: usize, kind: ListKind) -> Vec<String> {
    let recorder = handle as *mut c_void;
    let kind = kind as i32;
    // SAFETY: `recorder` is a live QAudioRecorder owned by the caller's
    // object; the answer is joined into one QString owned by this frame.
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

/// The input the recorder would use left to itself.
fn read_default_input(handle: usize) -> String {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    let name = cpp!(unsafe [recorder as "QAudioRecorder*"] -> QString as "QString" {
        return recorder->defaultAudioInput();
    });
    name.to_string()
}

/// Tell the recorder which input to record from.
fn set_input(handle: usize, input: &str) {
    let recorder = handle as *mut c_void;
    let input = QString::from(input);
    // SAFETY: as `read_list`; the QString is owned by this frame and
    // read by value.
    cpp!(unsafe [recorder as "QAudioRecorder*", input as "QString"] {
        recorder->setAudioInput(input);
    });
}

/// Pick the audio input from what the recorder offers.
///
/// The sound server by name where it is offered: on a device that is
/// `PulseAudio`, which is what the microphone is reached through. The
/// backend's own default is a `GStreamer` element that picks a source
/// for itself, and a recording made through it came out silent on a
/// phone. Failing that, the backend's default, and failing that the
/// first thing on the list. Nothing offered is `None`, and the recorder
/// is left alone.
fn choose_input(inputs: &[String], default: &str) -> Option<String> {
    inputs
        .iter()
        .find(|name| name.to_ascii_lowercase().contains("pulse"))
        .or_else(|| inputs.iter().find(|name| name.as_str() == default))
        .or_else(|| inputs.first())
        .cloned()
}

/// Ask the recorder to stop. The file is finished a moment later.
fn stop_recording(handle: usize) {
    let recorder = handle as *mut c_void;
    // SAFETY: as `read_list`.
    cpp!(unsafe [recorder as "QAudioRecorder*"] {
        recorder->stop();
    });
}

/// Milliseconds recorded, the status, the state, and the error message
/// when there is one.
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

/// The recorder's error message, empty when it has none.
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

/// Where the recorder wrote, once it has: the local path, or empty.
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
        // What Qt's capture plugin lists on a Sailfish phone: its own
        // names, FLAC and Ogg among them, and no AAC.
        let codecs = names(&["audio/vorbis", "audio/PCM", "audio/FLAC", "audio/opus"]);
        let containers = names(&["matroska", "ogg", "wav", "raw"]);
        assert_eq!(
            choose_format(&codecs, &containers),
            Some(format("audio/PCM", "wav"))
        );
        // MP3 offered too, where lamemp3enc is installed: still plain
        // samples, which are made into MP3 at a bit rate of this app's
        // choosing.
        let with_mp3 = names(&["audio/mpeg", "audio/PCM", "audio/FLAC"]);
        assert_eq!(
            choose_format(&with_mp3, &containers),
            Some(format("audio/PCM", "wav"))
        );
        // A build that reads GStreamer's own names.
        let gst = names(&["audio/x-flac", "audio/x-raw", "audio/x-opus"]);
        let gst_containers = names(&["application/ogg", "audio/x-wav"]);
        assert_eq!(
            choose_format(&gst, &gst_containers),
            Some(format("audio/x-raw", "audio/x-wav"))
        );
        // Encoders but no plain samples, or no WAV to put them in: nothing,
        // rather than a voice message iOS shows as a file.
        let encoders_only = names(&["audio/FLAC", "audio/opus", "audio/vorbis"]);
        assert_eq!(choose_format(&encoders_only, &containers), None);
        assert_eq!(choose_format(&codecs, &names(&["ogg", "raw"])), None);
        // Nothing offered, as headlessly: nothing chosen.
        assert_eq!(choose_format(&[], &containers), None);
        assert_eq!(choose_format(&codecs, &[]), None);
    }

    #[test]
    fn the_sound_server_is_the_input_where_it_is_offered() {
        // What Qt's GStreamer backend lists on a device: its own
        // automatic source first, then the sound server, then ALSA.
        let inputs = names(&["default:", "pulseaudio:", "alsa:hw:0,0"]);
        assert_eq!(
            choose_input(&inputs, "default:"),
            Some("pulseaudio:".to_string())
        );
        // No sound server: the backend's default, where it is listed.
        let without = names(&["alsa:hw:0,0", "default:"]);
        assert_eq!(
            choose_input(&without, "default:"),
            Some("default:".to_string())
        );
        // A default that is not on the list: the first thing that is.
        assert_eq!(
            choose_input(&without, "oss:"),
            Some("alsa:hw:0,0".to_string())
        );
        // Nothing at all, as headlessly: nothing chosen.
        assert_eq!(choose_input(&[], "default:"), None);
    }
}
