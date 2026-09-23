//! A voice message as MP3, made from the recording while it is recorded.
//!
//! What a voice message is recorded in decides who can play it. The
//! other clients agree: Android and iOS record AAC at 32 kbit/s, or 24 at
//! the lower outgoing media quality, and the desktop client -- whose
//! browser's own recorder offers nothing but Opus -- takes the raw
//! samples and encodes MP3 itself, with LAME. Opus is not an option for
//! the same reason it was not one there: the iOS client shows anything
//! that is `audio/ogg` as a file rather than as a voice message.
//!
//! Sailfish's recorder offers AAC nowhere, and MP3 only where
//! `lamemp3enc` is installed, which it is not on the phone. So this does
//! what the desktop client does: the recorder writes plain samples, as
//! WAV, and this reads them back as they land and encodes them to MP3 at
//! the other clients' bit rates, with LAME built into the app. The WAV is
//! scratch and is gone once the MP3 is finished.
//!
//! At a constant bit rate the size of the MP3 follows from its length,
//! which is what lets the recorder stop at the longest recording the
//! relay takes ([`longest_ms`]) rather than after one it would refuse.
//! The encoding keeps to that length too, whatever the recorder hands
//! over past it.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::num::NonZeroU32;
use std::path::PathBuf;

use mp3lame_encoder::{Bitrate, Builder, Encoder, FlushGap, Mode, MonoPcm, Quality, VbrMode};

/// Samples a second the recorder is asked for, and what the MP3 carries:
/// the voice band, and MPEG-2's own rate, so nothing is resampled on the
/// way when the recorder does as it is asked.
pub(crate) const SAMPLE_RATE: u32 = 16_000;

/// Kept below the relay's ceiling when working out the longest recording.
/// LAME's delay at the start and the frame it pads the end out to come to
/// a few hundred bytes; this is that, many times over.
const HEADROOM_BYTES: u64 = 16 * 1024;

/// How much of the front of the WAV is searched for where the samples
/// start. A header is some tens of bytes; a file this long with no
/// samples in it is not a recording.
const HEADER_SEARCH: u64 = 64 * 1024;

/// How much is read from the WAV at a time.
const READ_CHUNK: usize = 64 * 1024;

/// How many samples go to LAME at a time, which is what bounds the buffer
/// its output goes into.
const ENCODE_CHUNK: usize = 8192;

/// The MP3's bit rate at an outgoing media quality -- the core's
/// `media_quality`, 0 balanced and 1 less data -- in bits a second. The
/// other clients' own pair.
pub(crate) fn bit_rate(media_quality: u32) -> u32 {
    if media_quality == 1 {
        24_000
    } else {
        32_000
    }
}

/// LAME's name for a bit rate [`bit_rate`] gives.
fn lame_bit_rate(bits_per_second: u32) -> Bitrate {
    if bits_per_second <= 24_000 {
        Bitrate::Kbps24
    } else {
        Bitrate::Kbps32
    }
}

/// The longest recording, in milliseconds, whose MP3 fits in `limit_bytes`
/// at `bits_per_second`; 0 when there is no limit to fit, which is what a
/// limit of 0 -- the core has not said yet -- means everywhere else too.
///
/// Never 0 for a limit there is: a relay that takes almost nothing gets a
/// recording that stops almost at once, not one without an end.
pub(crate) fn longest_ms(limit_bytes: u64, bits_per_second: u32) -> u32 {
    if limit_bytes == 0 || bits_per_second == 0 {
        return 0;
    }
    let usable = limit_bytes.saturating_sub(HEADROOM_BYTES);
    let ms = usable.saturating_mul(8000) / u64::from(bits_per_second);
    u32::try_from(ms).unwrap_or(u32::MAX).max(1)
}

/// Whether the samples are whole numbers or floating point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Encoding {
    Int,
    Float,
}

/// What a WAV header says about the samples after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WavFormat {
    encoding: Encoding,
    channels: u16,
    sample_rate: u32,
    bits: u16,
    /// Where in the file the samples start.
    data_start: u64,
    /// How many bytes of samples the header says follow. A recorder
    /// writes this once it has finished, so it is only believed then.
    data_len: u64,
}

impl WavFormat {
    /// Bytes to one sample of every channel.
    fn frame_bytes(&self) -> usize {
        usize::from(self.channels) * usize::from(self.bits / 8)
    }
}

/// Read a WAV header off the front of `bytes`.
///
/// `Ok(None)` when not enough of it has been written yet to tell where
/// the samples start -- the recorder writes the file in pieces -- and an
/// error when what is there is not a WAV this can read.
fn parse_header(bytes: &[u8]) -> Result<Option<WavFormat>, String> {
    let not_wav = || "the recording is not a WAV file".to_string();
    if bytes.len() < 12 {
        return Ok(None);
    }
    if &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(not_wav());
    }
    let mut format: Option<(Encoding, u16, u32, u16)> = None;
    let mut at = 12_usize;
    loop {
        let (Some(id), Some(size)) = (
            at.checked_add(4).and_then(|end| bytes.get(at..end)),
            at.checked_add(4).and_then(|end| u32_at(bytes, end)),
        ) else {
            return Ok(None);
        };
        // Both reads above reached this far, so this cannot overflow.
        let body = at + 8;
        if id == b"data" {
            let Some((encoding, channels, sample_rate, bits)) = format else {
                return Err(not_wav());
            };
            return Ok(Some(WavFormat {
                encoding,
                channels,
                sample_rate,
                bits,
                data_start: body as u64,
                data_len: u64::from(size),
            }));
        }
        if id == b"fmt " {
            let (Some(tag), Some(channels), Some(sample_rate), Some(bits)) = (
                u16_at(bytes, body),
                u16_at(bytes, body + 2),
                u32_at(bytes, body + 4),
                u16_at(bytes, body + 14),
            ) else {
                return Ok(None);
            };
            // WAVE_FORMAT_EXTENSIBLE names the real format in the first
            // two bytes of the GUID that ends the chunk.
            let tag = if tag == 0xFFFE {
                let Some(real) = u16_at(bytes, body + 24) else {
                    return Ok(None);
                };
                real
            } else {
                tag
            };
            let encoding = match (tag, bits) {
                (1, 8 | 16 | 24 | 32) => Encoding::Int,
                (3, 32 | 64) => Encoding::Float,
                _ => {
                    return Err(format!(
                        "the recording is in a format this cannot read ({tag}, {bits} bits)"
                    ))
                }
            };
            if channels == 0 || sample_rate == 0 {
                return Err(not_wav());
            }
            format = Some((encoding, channels, sample_rate, bits));
        }
        // Chunks are padded to an even length.
        let padded = usize::try_from(size)
            .unwrap_or(usize::MAX)
            .saturating_add(1)
            & !1;
        at = body.saturating_add(padded);
    }
}

/// A little-endian `u16` at `at`, if the bytes reach that far.
fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let two = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([two[0], two[1]]))
}

/// A little-endian `u32` at `at`, if the bytes reach that far.
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let four = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([four[0], four[1], four[2], four[3]]))
}

/// Whole frames of `bytes` as one channel of 16-bit samples, the
/// channels averaged: what LAME is given.
fn to_mono(bytes: &[u8], format: &WavFormat, out: &mut Vec<i16>) {
    let width = usize::from(format.bits / 8);
    let channels = i64::from(format.channels);
    for frame in bytes.chunks_exact(format.frame_bytes()) {
        let sum: i64 = frame
            .chunks_exact(width)
            .map(|sample| i64::from(sample_value(sample, format.encoding)))
            .sum();
        let mean = (sum / channels).clamp(i64::from(i16::MIN), i64::from(i16::MAX));
        out.push(i16::try_from(mean).unwrap_or_default());
    }
}

/// One sample, on the scale of a 16-bit one.
fn sample_value(bytes: &[u8], encoding: Encoding) -> i32 {
    match (encoding, bytes) {
        // 8-bit WAV is unsigned, centred on 128.
        (Encoding::Int, [byte]) => (i32::from(*byte) - 128) << 8,
        // Little-endian, so the last two bytes of a wider sample are its
        // top sixteen bits.
        (Encoding::Int, [.., low, high]) => i32::from(i16::from_le_bytes([*low, *high])),
        (Encoding::Float, _) => match bytes.len() {
            4 => bytes
                .try_into()
                .map_or(0, |four| from_unit(f64::from(f32::from_le_bytes(four)))),
            8 => bytes
                .try_into()
                .map_or(0, |eight| from_unit(f64::from_le_bytes(eight))),
            _ => 0,
        },
        (Encoding::Int, []) => 0,
    }
}

/// A floating-point sample, nominally within ±1, on the 16-bit scale.
fn from_unit(value: f64) -> i32 {
    let scaled = (value * 32767.0).round();
    if scaled.is_nan() {
        return 0;
    }
    // Clamped to the 16-bit range first, so nothing is lost to the cast.
    #[allow(clippy::cast_possible_truncation)]
    {
        scaled.clamp(-32768.0, 32767.0) as i32
    }
}

/// LAME, set up for a voice message: one channel at [`SAMPLE_RATE`], at a
/// constant `bits_per_second`, from samples at `input_rate`.
fn lame(input_rate: u32, bits_per_second: u32) -> Result<Encoder, String> {
    let refused =
        |err: mp3lame_encoder::BuildError| format!("the MP3 encoder refused its settings: {err}");
    let mut builder =
        Builder::new().ok_or_else(|| "the MP3 encoder could not be made".to_string())?;
    builder.set_num_channels(1).map_err(refused)?;
    builder.set_sample_rate(input_rate).map_err(refused)?;
    builder
        .set_output_sample_rate(NonZeroU32::new(SAMPLE_RATE))
        .map_err(refused)?;
    builder.set_mode(Mode::Mono).map_err(refused)?;
    // Constant, so that the size follows from the length.
    builder.set_vbr_mode(VbrMode::Off).map_err(refused)?;
    builder
        .set_brate(lame_bit_rate(bits_per_second))
        .map_err(refused)?;
    // LAME's own default.
    builder.set_quality(Quality::VeryNice).map_err(refused)?;
    // No Xing frame: it is written into the start of a file once the rest
    // is known, which a file written as it is recorded never is.
    builder.set_to_write_vbr_tag(false).map_err(refused)?;
    builder.build().map_err(refused)
}

/// The samples once the header has been read: the encoder, and what has
/// been handed to it.
struct Stream {
    format: WavFormat,
    encoder: Encoder,
    /// Bytes of a frame whose rest has not been read yet.
    partial: Vec<u8>,
    /// Frames handed to the encoder so far.
    samples: u64,
    mono: Vec<i16>,
    out: Vec<u8>,
}

impl Stream {
    /// Encode `bytes`, the next samples in the file, into `mp3`, keeping
    /// to `max_samples` in all.
    fn feed(&mut self, bytes: &[u8], max_samples: u64, mp3: &mut File) -> Result<(), String> {
        self.partial.extend_from_slice(bytes);
        let frame = self.format.frame_bytes();
        let whole = self.partial.len() / frame * frame;
        self.mono.clear();
        to_mono(&self.partial[..whole], &self.format, &mut self.mono);
        self.partial.drain(..whole);

        let room = max_samples.saturating_sub(self.samples);
        let take = self
            .mono
            .len()
            .min(usize::try_from(room).unwrap_or(usize::MAX));
        self.samples += take as u64;
        for piece in self.mono[..take].chunks(ENCODE_CHUNK) {
            self.out.clear();
            self.out
                .reserve(mp3lame_encoder::max_required_buffer_size(piece.len()));
            self.encoder
                .encode_to_vec(MonoPcm(piece), &mut self.out)
                .map_err(|err| format!("the recording could not be encoded: {err}"))?;
            mp3.write_all(&self.out).map_err(write_failed)?;
        }
        Ok(())
    }

    /// Finish the last frame.
    fn flush(&mut self, mp3: &mut File) -> Result<(), String> {
        self.out.clear();
        // What LAME asks of a buffer for its last frames.
        self.out.reserve(7200);
        self.encoder
            .flush_to_vec::<FlushGap>(&mut self.out)
            .map_err(|err| format!("the recording could not be encoded: {err}"))?;
        mp3.write_all(&self.out).map_err(write_failed)?;
        mp3.flush().map_err(write_failed)
    }
}

/// What a failed write of the MP3 is reported as.
fn write_failed(err: std::io::Error) -> String {
    format!("the voice message could not be written: {err}")
}

/// What a failed read of the WAV is reported as.
fn read_failed(err: std::io::Error) -> String {
    format!("the recording could not be read: {err}")
}

/// Turns the WAV the recorder is writing into the MP3 that is sent, a
/// piece at a time.
///
/// Made when recording starts, [`Self::pump`]ed while it runs, and then
/// either [`Self::finish`]ed once the recorder has closed the WAV or
/// [`Self::discard`]ed. Either way the WAV is removed.
pub(crate) struct Transcoder {
    wav_path: PathBuf,
    mp3_path: PathBuf,
    /// The WAV, once the recorder has made it.
    wav: Option<File>,
    mp3: File,
    bits_per_second: u32,
    /// The largest the MP3 may be; 0 for no limit.
    limit_bytes: u64,
    /// The samples, once the header has been read.
    stream: Option<Stream>,
    /// Bytes of samples read so far, counted from where they start.
    consumed: u64,
}

impl Transcoder {
    /// Start an MP3 at `mp3_path` from the WAV that is to be written at
    /// `wav_path`, at `bits_per_second`, no larger than `limit_bytes` (0
    /// for no limit).
    pub(crate) fn new(
        wav_path: PathBuf,
        mp3_path: PathBuf,
        bits_per_second: u32,
        limit_bytes: u64,
    ) -> Result<Self, String> {
        let mp3 = File::create(&mp3_path).map_err(write_failed)?;
        Ok(Self {
            wav_path,
            mp3_path,
            wav: None,
            mp3,
            bits_per_second,
            limit_bytes,
            stream: None,
            consumed: 0,
        })
    }

    /// The longest the recording may be, in milliseconds; 0 for no limit.
    pub(crate) fn limit_ms(&self) -> u32 {
        longest_ms(self.limit_bytes, self.bits_per_second)
    }

    /// Hold the MP3 to `limit_bytes` from here on. For a limit that is
    /// learnt after recording started: what is already encoded is short
    /// of any limit the recorder has not already stopped at.
    pub(crate) fn set_limit_bytes(&mut self, limit_bytes: u64) {
        self.limit_bytes = limit_bytes;
    }

    /// Read the WAV from `path` instead, when the recorder put it
    /// somewhere other than where it was asked to. Only while nothing has
    /// been found where it was expected: once samples have been read from
    /// there, that is where the recording is.
    pub(crate) fn follow(&mut self, path: PathBuf) {
        if self.stream.is_none() && path != self.wav_path {
            let _ = std::fs::remove_file(&self.wav_path);
            self.wav_path = path;
            self.wav = None;
        }
    }

    /// The most samples to encode, at the WAV's own rate.
    fn max_samples(&self, sample_rate: u32) -> u64 {
        match self.limit_ms() {
            0 => u64::MAX,
            ms => u64::from(ms) * u64::from(sample_rate) / 1000,
        }
    }

    /// Open the WAV and read its header, as far as the recorder has got:
    /// the rate of its samples once both are done, `None` until then.
    fn open(&mut self) -> Result<Option<u32>, String> {
        if self.wav.is_none() {
            match File::open(&self.wav_path) {
                Ok(file) => self.wav = Some(file),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(err) => return Err(read_failed(err)),
            }
        }
        if self.stream.is_none() {
            let Some(wav) = self.wav.as_mut() else {
                return Ok(None);
            };
            let Some(format) = read_header(wav)? else {
                return Ok(None);
            };
            self.stream = Some(Stream {
                format,
                encoder: lame(format.sample_rate, self.bits_per_second)?,
                partial: Vec::new(),
                samples: 0,
                mono: Vec::new(),
                out: Vec::new(),
            });
        }
        Ok(self.stream.as_ref().map(|stream| stream.format.sample_rate))
    }

    /// Encode whatever the recorder has written since the last call.
    ///
    /// Nothing at all until the WAV exists and its header is written: the
    /// recorder writes the file in pieces, the first of them a moment
    /// after it starts.
    pub(crate) fn pump(&mut self) -> Result<(), String> {
        self.pump_to(None)
    }

    /// [`Self::pump`], stopping at `end` bytes of samples when given.
    fn pump_to(&mut self, end: Option<u64>) -> Result<(), String> {
        let Some(sample_rate) = self.open()? else {
            return Ok(());
        };
        let max_samples = self.max_samples(sample_rate);
        let (Some(wav), Some(stream)) = (self.wav.as_mut(), self.stream.as_mut()) else {
            return Ok(());
        };
        wav.seek(SeekFrom::Start(stream.format.data_start + self.consumed))
            .map_err(read_failed)?;
        let mut chunk = vec![0_u8; READ_CHUNK];
        loop {
            let want = end.map_or(READ_CHUNK, |end| {
                usize::try_from(end.saturating_sub(self.consumed))
                    .unwrap_or(usize::MAX)
                    .min(READ_CHUNK)
            });
            if want == 0 {
                break;
            }
            let got = wav.read(&mut chunk[..want]).map_err(read_failed)?;
            if got == 0 {
                break;
            }
            self.consumed += got as u64;
            stream.feed(&chunk[..got], max_samples, &mut self.mp3)?;
        }
        Ok(())
    }

    /// Encode the rest, once the recorder has finished the WAV, and finish
    /// the MP3.
    ///
    /// Answers with how many samples the MP3 holds, at the WAV's rate, or
    /// `None` when nothing was recorded, in which case the MP3 is gone
    /// too. The WAV is removed whatever the answer.
    pub(crate) fn finish(mut self) -> Result<Option<u64>, String> {
        let result = self.finish_mp3();
        let _ = std::fs::remove_file(&self.wav_path);
        if !matches!(result, Ok(Some(_))) {
            let _ = std::fs::remove_file(&self.mp3_path);
        }
        result
    }

    /// [`Self::finish`], but for the removing.
    fn finish_mp3(&mut self) -> Result<Option<u64>, String> {
        // A finished WAV says how long its samples are, and anything after
        // them is the recorder's own -- tags, say -- and not sound.
        let end = match (self.wav.as_mut(), self.stream.as_ref()) {
            (Some(wav), Some(stream)) => final_data_len(wav, &stream.format)?,
            _ => None,
        };
        self.pump_to(end)?;
        let Some(stream) = self.stream.as_mut() else {
            return Ok(None);
        };
        if stream.samples == 0 {
            return Ok(None);
        }
        stream.flush(&mut self.mp3)?;
        Ok(Some(stream.samples))
    }

    /// Stop, and remove both files.
    pub(crate) fn discard(self) {
        let _ = std::fs::remove_file(&self.wav_path);
        let _ = std::fs::remove_file(&self.mp3_path);
    }
}

/// The header at the front of the WAV, once enough of it is written.
fn read_header(wav: &mut File) -> Result<Option<WavFormat>, String> {
    wav.seek(SeekFrom::Start(0)).map_err(read_failed)?;
    let mut head = Vec::new();
    wav.take(HEADER_SEARCH)
        .read_to_end(&mut head)
        .map_err(read_failed)?;
    match parse_header(&head)? {
        Some(format) => Ok(Some(format)),
        None if head.len() as u64 >= HEADER_SEARCH => Err("the recording is not a WAV file".into()),
        None => Ok(None),
    }
}

/// How many bytes of samples a finished WAV holds, when its header says
/// so believably: the recorder writes a placeholder while it records and
/// the real length when it closes the file.
fn final_data_len(wav: &mut File, format: &WavFormat) -> Result<Option<u64>, String> {
    let Some(header) = read_header(wav)? else {
        return Ok(None);
    };
    let file_len = wav.metadata().map_err(read_failed)?.len();
    let believable = header.data_start == format.data_start
        && header.data_len != 0
        && header.data_len != u64::from(u32::MAX)
        && header.data_start + header.data_len <= file_len;
    Ok(believable.then_some(header.data_len))
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use super::{
        bit_rate, longest_ms, parse_header, to_mono, Encoding, Transcoder, WavFormat,
        HEADROOM_BYTES, SAMPLE_RATE,
    };

    /// A canonical 44-byte header. `data_len` is what the recorder writes
    /// once it has finished; 0 is the placeholder while it records.
    fn header(channels: u16, rate: u32, bits: u16, tag: u16, data_len: u32) -> Vec<u8> {
        let block = channels * (bits / 8);
        let mut head = Vec::new();
        head.extend_from_slice(b"RIFF");
        head.extend_from_slice(&(36 + data_len).to_le_bytes());
        head.extend_from_slice(b"WAVE");
        head.extend_from_slice(b"fmt ");
        head.extend_from_slice(&16_u32.to_le_bytes());
        head.extend_from_slice(&tag.to_le_bytes());
        head.extend_from_slice(&channels.to_le_bytes());
        head.extend_from_slice(&rate.to_le_bytes());
        head.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
        head.extend_from_slice(&block.to_le_bytes());
        head.extend_from_slice(&bits.to_le_bytes());
        head.extend_from_slice(b"data");
        head.extend_from_slice(&data_len.to_le_bytes());
        head
    }

    /// `milliseconds` of a 440 Hz tone, one channel of 16-bit samples.
    fn tone(rate: u32, milliseconds: u32) -> Vec<u8> {
        (0..rate * milliseconds / 1000)
            .flat_map(|n| {
                let phase = 2.0 * std::f64::consts::PI * 440.0 * f64::from(n) / f64::from(rate);
                i16::try_from(super::from_unit(phase.sin() * 0.4))
                    .unwrap()
                    .to_le_bytes()
            })
            .collect()
    }

    /// The frames of an MP3: bit rate in kbit/s, sample rate, and whether
    /// it is one channel, for each.
    fn frames(mp3: &[u8]) -> Vec<(u32, u32, bool)> {
        // MPEG-2 layer III, which is every rate this writes.
        const KBPS: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
        const RATES: [u32; 3] = [22_050, 24_000, 16_000];
        let mut found = Vec::new();
        let mut at = 0;
        while at + 4 <= mp3.len() {
            let head = &mp3[at..at + 4];
            assert_eq!(head[0], 0xFF, "no frame sync at {at}");
            assert_eq!(
                head[1] & 0xFE,
                0xF2,
                "not MPEG-2 layer III at {at}: {head:02x?}"
            );
            let kbps = KBPS[usize::from(head[2] >> 4)];
            let rate = RATES[usize::from((head[2] >> 2) & 3)];
            let padding = u32::from((head[2] >> 1) & 1);
            let mono = head[3] >> 6 == 3;
            found.push((kbps, rate, mono));
            at += (72 * kbps * 1000 / rate + padding) as usize;
        }
        assert_eq!(at, mp3.len(), "the last frame is cut short");
        found
    }

    /// A directory of the test's own.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("piirit-voice-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("make the scratch directory");
        dir
    }

    fn append(path: &Path, bytes: &[u8]) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open the WAV");
        file.write_all(bytes).expect("write the WAV");
    }

    #[test]
    fn the_bit_rates_are_the_other_clients() {
        assert_eq!(bit_rate(0), 32_000);
        assert_eq!(bit_rate(1), 24_000);
        // Anything the core does not know is balanced, as it is there.
        assert_eq!(bit_rate(7), 32_000);
    }

    #[test]
    fn the_longest_recording_is_what_the_limit_holds_at_the_bit_rate() {
        // The real core's recommendation: (30 - 1) MiB * 3/4.
        let limit = (30 - 1) * 1024 * 1024 / 4 * 3;
        assert_eq!(longest_ms(limit, 32_000), 5_697_536, "94:57");
        assert_eq!(longest_ms(limit, 24_000), 7_596_714, "126:36");
        // At 32 kbit/s every second is 4000 bytes, so what the limit holds
        // beyond the headroom is the length.
        assert_eq!(longest_ms(HEADROOM_BYTES + 4000, 32_000), 1000);
        // No limit known: no limit.
        assert_eq!(longest_ms(0, 32_000), 0);
        // A limit that holds nothing still ends the recording.
        assert_eq!(longest_ms(1024, 32_000), 1);
    }

    #[test]
    fn a_header_is_read_as_far_as_it_is_written() {
        let head = header(1, 16_000, 16, 1, 0);
        for cut in [0, 11, 20, 36, 40] {
            assert_eq!(parse_header(&head[..cut]), Ok(None), "cut at {cut}");
        }
        assert_eq!(
            parse_header(&head),
            Ok(Some(WavFormat {
                encoding: Encoding::Int,
                channels: 1,
                sample_rate: 16_000,
                bits: 16,
                data_start: 44,
                data_len: 0,
            }))
        );
    }

    #[test]
    fn a_chunk_before_the_samples_is_stepped_over() {
        let mut head = header(2, 48_000, 16, 1, 8);
        // A LIST chunk of odd length between the format and the samples,
        // padded to even as RIFF has it.
        let data = head.split_off(36);
        head.extend_from_slice(b"LIST");
        head.extend_from_slice(&5_u32.to_le_bytes());
        head.extend_from_slice(b"INFO\0\0");
        head.extend_from_slice(&data);
        let format = parse_header(&head).unwrap().unwrap();
        assert_eq!(format.data_start, 36 + 14 + 8);
        assert_eq!(format.data_len, 8);
        assert_eq!(format.channels, 2);
        assert_eq!(format.sample_rate, 48_000);
    }

    #[test]
    fn an_extensible_header_says_its_format_in_the_guid() {
        let mut head = Vec::new();
        head.extend_from_slice(b"RIFF\0\0\0\0WAVEfmt ");
        head.extend_from_slice(&40_u32.to_le_bytes());
        head.extend_from_slice(&0xFFFE_u16.to_le_bytes());
        head.extend_from_slice(&1_u16.to_le_bytes());
        head.extend_from_slice(&16_000_u32.to_le_bytes());
        head.extend_from_slice(&64_000_u32.to_le_bytes());
        head.extend_from_slice(&4_u16.to_le_bytes());
        head.extend_from_slice(&32_u16.to_le_bytes());
        head.extend_from_slice(&22_u16.to_le_bytes());
        head.extend_from_slice(&32_u16.to_le_bytes());
        head.extend_from_slice(&4_u32.to_le_bytes());
        // KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: format 3.
        head.extend_from_slice(&3_u16.to_le_bytes());
        head.extend_from_slice(&[0; 14]);
        head.extend_from_slice(b"data\0\0\0\0");
        let format = parse_header(&head).unwrap().unwrap();
        assert_eq!(format.encoding, Encoding::Float);
        assert_eq!(format.bits, 32);
        assert_eq!(format.data_start, head.len() as u64);
    }

    #[test]
    fn what_is_not_a_readable_wav_is_said_so() {
        assert!(parse_header(b"OggS\0\0\0\0\0\0\0\0\0\0").is_err());
        // The samples before any word of what they are.
        assert!(parse_header(b"RIFF\0\0\0\0WAVEdata\0\0\0\0").is_err());
        // Twelve-bit samples, which nothing writes.
        assert!(parse_header(&header(1, 16_000, 12, 1, 0)).is_err());
        // Compressed WAV, format 2 (ADPCM).
        assert!(parse_header(&header(1, 16_000, 16, 2, 0)).is_err());
    }

    #[test]
    fn every_sample_format_comes_out_as_one_channel_of_sixteen_bits() {
        let format = |channels, bits, encoding| WavFormat {
            encoding,
            channels,
            sample_rate: 16_000,
            bits,
            data_start: 44,
            data_len: 0,
        };
        let mut out = Vec::new();

        // Two channels, averaged; a trailing partial frame left alone.
        let stereo: Vec<u8> = [1000_i16, 3000, -2000, -4000]
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .chain([0x7F])
            .collect();
        to_mono(&stereo, &format(2, 16, Encoding::Int), &mut out);
        assert_eq!(out, [2000, -3000]);

        out.clear();
        to_mono(&[128, 255, 0], &format(1, 8, Encoding::Int), &mut out);
        assert_eq!(out, [0, 127 << 8, -128 << 8]);

        out.clear();
        let wide: Vec<u8> = [0x00, 0x34, 0x12, 0xFF, 0x00, 0x80].to_vec();
        to_mono(&wide, &format(1, 24, Encoding::Int), &mut out);
        assert_eq!(out, [0x1234, i16::MIN]);

        out.clear();
        let full: Vec<u8> = (0x1234_5678_i32).to_le_bytes().to_vec();
        to_mono(&full, &format(1, 32, Encoding::Int), &mut out);
        assert_eq!(out, [0x1234]);

        out.clear();
        let floats: Vec<u8> = [0.5_f32, -1.0, 2.0, f32::NAN]
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        to_mono(&floats, &format(1, 32, Encoding::Float), &mut out);
        assert_eq!(out, [16384, -32767, 32767, 0]);
    }

    #[test]
    fn a_wav_written_in_pieces_becomes_an_mp3_at_the_bit_rate() {
        for (media_quality, kbps) in [(0, 32), (1, 24)] {
            let dir = scratch(&format!("pieces-{media_quality}"));
            let wav = dir.join("voice.wav");
            let mp3 = dir.join("voice.mp3");
            let mut transcoder =
                Transcoder::new(wav.clone(), mp3.clone(), bit_rate(media_quality), 0).unwrap();

            // Before the recorder has made the file, and while it has
            // written only part of the header: nothing yet, and no error.
            transcoder.pump().unwrap();
            let head = header(1, SAMPLE_RATE, 16, 1, 0);
            append(&wav, &head[..20]);
            transcoder.pump().unwrap();
            append(&wav, &head[20..]);

            // Two seconds, landing in uneven pieces -- one of them splitting
            // a sample -- as a recorder's buffered writes do.
            let sound = tone(SAMPLE_RATE, 2000);
            for piece in sound.chunks(12_345) {
                append(&wav, piece);
                transcoder.pump().unwrap();
            }
            let samples = transcoder.finish().unwrap();
            assert_eq!(samples, Some(2 * u64::from(SAMPLE_RATE)));

            assert!(!wav.exists(), "the scratch WAV was left behind");
            let bytes = std::fs::read(&mp3).unwrap();
            let found = frames(&bytes);
            assert!(
                found
                    .iter()
                    .all(|frame| *frame == (kbps, SAMPLE_RATE, true)),
                "not every frame is {kbps} kbit/s, 16 kHz, one channel: {found:?}"
            );
            // Two seconds, and the encoder's own delay and padding: a few
            // frames of 576 samples more, never fewer.
            let milliseconds = found.len() * 576 * 1000 / 16_000;
            assert!(
                (2000..2200).contains(&milliseconds),
                "{milliseconds} ms in {} frames",
                found.len()
            );
            assert_eq!(
                bytes.len(),
                found.len() * 72 * kbps as usize * 1000 / 16_000
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn the_mp3_never_runs_past_the_limit() {
        let dir = scratch("limit");
        let wav = dir.join("voice.wav");
        let mp3 = dir.join("voice.mp3");
        // Half a second's worth at 32 kbit/s.
        let limit = HEADROOM_BYTES + 2000;
        let mut transcoder = Transcoder::new(wav.clone(), mp3.clone(), 32_000, limit).unwrap();
        assert_eq!(transcoder.limit_ms(), 500);

        // Three seconds recorded -- the recorder does not stop to the
        // sample -- and only the first half second of it kept.
        append(&wav, &header(1, SAMPLE_RATE, 16, 1, 0));
        append(&wav, &tone(SAMPLE_RATE, 3000));
        transcoder.pump().unwrap();
        assert_eq!(
            transcoder.finish().unwrap(),
            Some(u64::from(SAMPLE_RATE) / 2)
        );
        let size = std::fs::metadata(&mp3).unwrap().len();
        assert!(size <= limit, "{size} bytes against a limit of {limit}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_limit_learnt_while_recording_holds_from_then_on() {
        let dir = scratch("late-limit");
        let wav = dir.join("voice.wav");
        let mp3 = dir.join("voice.mp3");
        let mut transcoder = Transcoder::new(wav.clone(), mp3, 32_000, 0).unwrap();
        assert_eq!(transcoder.limit_ms(), 0);
        append(&wav, &header(1, SAMPLE_RATE, 16, 1, 0));
        append(&wav, &tone(SAMPLE_RATE, 250));
        transcoder.pump().unwrap();
        transcoder.set_limit_bytes(HEADROOM_BYTES + 4000);
        assert_eq!(transcoder.limit_ms(), 1000);
        append(&wav, &tone(SAMPLE_RATE, 2000));
        assert_eq!(transcoder.finish().unwrap(), Some(u64::from(SAMPLE_RATE)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn what_follows_the_samples_in_a_finished_wav_is_not_encoded() {
        let dir = scratch("trailer");
        let wav = dir.join("voice.wav");
        let mp3 = dir.join("voice.mp3");
        let mut transcoder = Transcoder::new(wav.clone(), mp3, 32_000, 0).unwrap();
        let sound = tone(SAMPLE_RATE, 1000);
        append(&wav, &header(1, SAMPLE_RATE, 16, 1, 0));
        append(&wav, &sound[..10_000]);
        transcoder.pump().unwrap();
        append(&wav, &sound[10_000..]);
        // As the recorder closes the file: the real length into the
        // header, and a chunk of tags after the samples.
        let finished = header(1, SAMPLE_RATE, 16, 1, u32::try_from(sound.len()).unwrap());
        let mut file = std::fs::OpenOptions::new().write(true).open(&wav).unwrap();
        file.write_all(&finished).unwrap();
        drop(file);
        append(&wav, b"LIST\x08\0\0\0INFOjunk");
        assert_eq!(transcoder.finish().unwrap(), Some(u64::from(SAMPLE_RATE)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wav_the_recorder_put_elsewhere_is_followed_there() {
        let dir = scratch("elsewhere");
        let asked = dir.join("voice.mp3.wav");
        let actual = dir.join("voice.mp3.wav.wav");
        let mp3 = dir.join("voice.mp3");
        let mut transcoder = Transcoder::new(asked.clone(), mp3.clone(), 32_000, 0).unwrap();
        append(&actual, &header(1, SAMPLE_RATE, 16, 1, 0));
        append(&actual, &tone(SAMPLE_RATE, 1000));
        // Nothing where it was asked to go.
        transcoder.pump().unwrap();
        transcoder.follow(actual.clone());
        assert_eq!(transcoder.finish().unwrap(), Some(u64::from(SAMPLE_RATE)));
        assert!(mp3.exists() && !actual.exists() && !asked.exists());

        // Once samples have been read from where it was asked to go, that
        // is where the recording is.
        let mut transcoder = Transcoder::new(asked.clone(), mp3.clone(), 32_000, 0).unwrap();
        append(&asked, &header(1, SAMPLE_RATE, 16, 1, 0));
        append(&asked, &tone(SAMPLE_RATE, 500));
        transcoder.pump().unwrap();
        transcoder.follow(dir.join("somewhere-else.wav"));
        assert_eq!(
            transcoder.finish().unwrap(),
            Some(u64::from(SAMPLE_RATE) / 2)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_recording_at_another_rate_is_still_a_16_khz_mp3() {
        let dir = scratch("resampled");
        let wav = dir.join("voice.wav");
        let mp3 = dir.join("voice.mp3");
        let transcoder = Transcoder::new(wav.clone(), mp3.clone(), 32_000, 0).unwrap();
        append(&wav, &header(1, 48_000, 16, 1, 0));
        append(&wav, &tone(48_000, 1000));
        assert_eq!(transcoder.finish().unwrap(), Some(48_000));
        let found = frames(&std::fs::read(&mp3).unwrap());
        assert!(
            found.iter().all(|frame| *frame == (32, SAMPLE_RATE, true)),
            "{found:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_recorded_leaves_nothing_behind() {
        let dir = scratch("nothing");
        let wav = dir.join("voice.wav");
        let mp3 = dir.join("voice.mp3");

        // The recorder never made the file.
        let transcoder = Transcoder::new(wav.clone(), mp3.clone(), 32_000, 0).unwrap();
        assert!(mp3.exists());
        assert_eq!(transcoder.finish().unwrap(), None);
        assert!(!mp3.exists());

        // It made one with a header and no sound in it.
        let transcoder = Transcoder::new(wav.clone(), mp3.clone(), 32_000, 0).unwrap();
        append(&wav, &header(1, SAMPLE_RATE, 16, 1, 0));
        assert_eq!(transcoder.finish().unwrap(), None);
        assert!(!mp3.exists() && !wav.exists());

        // It wrote something that is not a WAV.
        let transcoder = Transcoder::new(wav.clone(), mp3.clone(), 32_000, 0).unwrap();
        append(&wav, b"this is not a recording at all");
        assert!(transcoder.finish().is_err());
        assert!(!mp3.exists() && !wav.exists());

        // Cancelled halfway.
        let mut transcoder = Transcoder::new(wav.clone(), mp3.clone(), 32_000, 0).unwrap();
        append(&wav, &header(1, SAMPLE_RATE, 16, 1, 0));
        append(&wav, &tone(SAMPLE_RATE, 1000));
        transcoder.pump().unwrap();
        transcoder.discard();
        assert!(!mp3.exists() && !wav.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
