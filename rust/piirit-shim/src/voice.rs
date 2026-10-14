//! Voice message as MP3, encoded as recorded.
//!
//! Android/iOS record AAC at 32 kbit/s (24 at lower media quality); desktop encodes MP3 itself
//! with LAME. Not Opus: iOS shows `audio/ogg` as file. Sailfish recorder has no AAC, MP3 only
//! with `lamemp3enc` (absent on phone). So as desktop: recorder writes WAV, built-in LAME encodes
//! it as it lands at the other clients' rates. WAV is scratch, removed after.
//!
//! CBR: size follows length, so recorder stops at [`longest_ms`], not past relay limit; encoding
//! keeps to that length whatever recorder hands over after.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use mp3lame_encoder::{Bitrate, Builder, Encoder, FlushGap, Mode, MonoPcm, Quality, VbrMode};

/// Recorder and MP3 rate: voice band, MPEG-2's own rate, so no resample.
pub(crate) const SAMPLE_RATE: u32 = 16_000;

/// Headroom for LAME delay and padding.
const HEADROOM_BYTES: u64 = 16 * 1024;

/// Header search limit: header is tens of bytes; this much without samples is no recording.
const HEADER_SEARCH: u64 = 64 * 1024;

/// Read size.
const READ_CHUNK: usize = 64 * 1024;

/// Samples per LAME call; bounds output buffer.
const ENCODE_CHUNK: usize = 8192;

/// bit/s at `media_quality` (0 balanced, 1 less data): other clients' pair.
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

/// Longest recording in ms whose MP3 fits `limit_bytes`; 0 for no limit (limit 0 = core hasn't
/// said).
///
/// Never 0 for a real limit: tiny limit stops almost at once, not never.
pub(crate) fn longest_ms(limit_bytes: u64, bits_per_second: u32) -> u32 {
    if limit_bytes == 0 || bits_per_second == 0 {
        return 0;
    }
    let usable = limit_bytes.saturating_sub(HEADROOM_BYTES);
    let ms = usable
        .saturating_mul(8000)
        .checked_div(u64::from(bits_per_second))
        .unwrap_or(0);
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
    /// Sample bytes per header; trusted only once finished.
    data_len: u64,
}

impl WavFormat {
    /// Bytes per frame.
    fn frame_bytes(&self) -> usize {
        usize::from(self.channels).saturating_mul(usize::from(self.bits / 8))
    }
}

/// Read WAV header off front of `bytes`.
///
/// `Ok(None)`: not written enough yet. `Err`: not a WAV.
fn parse_header(bytes: &[u8]) -> Result<Option<WavFormat>, String> {
    let not_wav = || "the recording is not a WAV file".to_string();
    if bytes.len() < 12 {
        return Ok(None);
    }
    if !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WAVE".as_slice()) {
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
        let body = at.saturating_add(8);
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
                u16_at(bytes, body.saturating_add(2)),
                u32_at(bytes, body.saturating_add(4)),
                u16_at(bytes, body.saturating_add(14)),
            ) else {
                return Ok(None);
            };
            // EXTENSIBLE: real tag in GUID.
            let tag = if tag == 0xFFFE {
                let Some(real) = u16_at(bytes, body.saturating_add(24)) else {
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
        // Chunks pad to even length.
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
    Some(u16::from_le_bytes(two.try_into().ok()?))
}

/// A little-endian `u32` at `at`, if the bytes reach that far.
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let four = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(four.try_into().ok()?))
}

/// Frames as 16-bit mono, channels averaged.
fn to_mono(bytes: &[u8], format: &WavFormat, out: &mut Vec<i16>) {
    let width = usize::from(format.bits / 8);
    let frame_bytes = format.frame_bytes();
    // `chunks_exact` rejects 0.
    if width == 0 || frame_bytes == 0 {
        return;
    }
    let channels = i64::from(format.channels);
    for frame in bytes.chunks_exact(frame_bytes) {
        let sum: i64 = frame
            .chunks_exact(width)
            .map(|sample| i64::from(sample_value(sample, format.encoding)))
            .sum();
        // Not 0: else `frame_bytes` would be.
        let mean = sum
            .checked_div(channels)
            .unwrap_or(0)
            .clamp(i64::from(i16::MIN), i64::from(i16::MAX));
        out.push(i16::try_from(mean).unwrap_or_default());
    }
}

/// One sample, on the scale of a 16-bit one.
fn sample_value(bytes: &[u8], encoding: Encoding) -> i32 {
    match (encoding, bytes) {
        // 8-bit unsigned: flip top bit.
        (Encoding::Int, [byte]) => i32::from(i16::from_le_bytes([0, *byte ^ 0x80])),
        // Little-endian: top 16 bits last.
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

/// Float sample, nominally +-1.
fn from_unit(value: f64) -> i32 {
    let scaled = (value * 32767.0).round();
    if scaled.is_nan() {
        return 0;
    }
    // Clamp first: no loss in cast.
    #[allow(clippy::cast_possible_truncation)]
    {
        scaled.clamp(-32768.0, 32767.0) as i32
    }
}

/// LAME: mono, CBR `bits_per_second`.
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
    // CBR: size follows length.
    builder.set_vbr_mode(VbrMode::Off).map_err(refused)?;
    builder
        .set_brate(lame_bit_rate(bits_per_second))
        .map_err(refused)?;
    builder.set_quality(Quality::VeryNice).map_err(refused)?;
    // No Xing frame: can't rewrite while recording.
    builder.set_to_write_vbr_tag(false).map_err(refused)?;
    builder.build().map_err(refused)
}

/// Samples once header read.
struct Stream {
    format: WavFormat,
    encoder: Encoder,
    /// Partial frame bytes.
    partial: Vec<u8>,
    /// Frames sent to encoder.
    samples: u64,
    mono: Vec<i16>,
    out: Vec<u8>,
}

impl Stream {
    /// Encode next `bytes` into `mp3`, up to `max_samples`.
    fn feed(&mut self, bytes: &[u8], max_samples: u64, mp3: &mut File) -> Result<(), String> {
        self.partial.extend_from_slice(bytes);
        let frame = self.format.frame_bytes();
        // `frame` is never 0 for a header `parse_header` read.
        let whole = self
            .partial
            .len()
            .checked_div(frame)
            .map_or(0, |frames| frames.saturating_mul(frame));
        self.mono.clear();
        // `to_mono` leaves partial frame.
        to_mono(&self.partial, &self.format, &mut self.mono);
        self.partial.drain(..whole);

        let room = max_samples.saturating_sub(self.samples);
        let take = self
            .mono
            .len()
            .min(usize::try_from(room).unwrap_or(usize::MAX));
        self.samples = self.samples.saturating_add(take as u64);
        self.mono.truncate(take);
        for piece in self.mono.chunks(ENCODE_CHUNK) {
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
        // LAME flush buffer.
        self.out.reserve(7200);
        self.encoder
            .flush_to_vec::<FlushGap>(&mut self.out)
            .map_err(|err| format!("the recording could not be encoded: {err}"))?;
        mp3.write_all(&self.out).map_err(write_failed)?;
        mp3.flush().map_err(write_failed)
    }
}

/// Write error text.
fn write_failed(err: std::io::Error) -> String {
    format!("the voice message could not be written: {err}")
}

/// Read error text.
fn read_failed(err: std::io::Error) -> String {
    format!("the recording could not be read: {err}")
}

/// WAV being written, to MP3 that is sent.
///
/// `pump`ed while recording; `finish`ed or `discard`ed. WAV removed either way.
pub(crate) struct Transcoder {
    wav_path: PathBuf,
    mp3_path: PathBuf,
    /// WAV, once recorder made it.
    wav: Option<File>,
    mp3: File,
    bits_per_second: u32,
    /// Max MP3 bytes; 0 no limit.
    limit_bytes: u64,
    /// Samples, once header read.
    stream: Option<Stream>,
    /// Sample bytes read so far.
    consumed: u64,
}

impl Transcoder {
    /// MP3 at `mp3_path` from WAV to be written at `wav_path`, at `bits_per_second`, at most
    /// `limit_bytes` (0: no limit).
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

    /// Longest recording, ms; 0 no limit.
    pub(crate) fn limit_ms(&self) -> u32 {
        longest_ms(self.limit_bytes, self.bits_per_second)
    }

    /// Limit from here on, for one learnt after start: what's encoded is already under any
    /// limit recorder hasn't stopped at.
    pub(crate) fn set_limit_bytes(&mut self, limit_bytes: u64) {
        self.limit_bytes = limit_bytes;
    }

    /// Read WAV from `path` if recorder wrote elsewhere, before samples read.
    pub(crate) fn follow(&mut self, path: PathBuf) {
        if self.stream.is_none() && path != self.wav_path {
            remove_scratch(&self.wav_path);
            self.wav_path = path;
            self.wav = None;
        }
    }

    /// Most samples to encode.
    fn max_samples(&self, sample_rate: u32) -> u64 {
        match self.limit_ms() {
            0 => u64::MAX,
            // Two u32s: product fits.
            ms => u64::from(ms).saturating_mul(u64::from(sample_rate)) / 1000,
        }
    }

    /// Open WAV, read header; rate once done.
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

    /// Encode what recorder wrote since last call.
    ///
    /// Nothing until WAV and header exist.
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
        wav.seek(SeekFrom::Start(
            stream.format.data_start.saturating_add(self.consumed),
        ))
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
            // `want` is at most `READ_CHUNK`, and `got` at most `want`.
            let Some(buffer) = chunk.get_mut(..want) else {
                break;
            };
            let got = wav.read(buffer).map_err(read_failed)?;
            let Some(read) = buffer.get(..got).filter(|read| !read.is_empty()) else {
                break;
            };
            self.consumed = self.consumed.saturating_add(got as u64);
            stream.feed(read, max_samples, &mut self.mp3)?;
        }
        Ok(())
    }

    /// Encode rest, finish MP3, once WAV finished.
    ///
    /// Returns MP3 sample count at WAV rate; `None` if nothing recorded (MP3 removed). WAV
    /// removed either way.
    pub(crate) fn finish(mut self) -> Result<Option<u64>, String> {
        let result = self.finish_mp3();
        remove_scratch(&self.wav_path);
        if !matches!(result, Ok(Some(_))) {
            remove_scratch(&self.mp3_path);
        }
        result
    }

    /// [`Self::finish`] body.
    fn finish_mp3(&mut self) -> Result<Option<u64>, String> {
        // Finished WAV: length from header; tags after aren't sound.
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
        remove_scratch(&self.wav_path);
        remove_scratch(&self.mp3_path);
    }
}

/// Remove file. Missing: silent. Other error: logged.
fn remove_scratch(path: &Path) {
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            eprintln!("piirit: cannot remove {}: {err}", path.display());
        }
        _ => {}
    }
}

/// Header at WAV front.
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

/// Sample bytes of finished WAV, if header believable: placeholder while recording, real length
/// on close.
fn final_data_len(wav: &mut File, format: &WavFormat) -> Result<Option<u64>, String> {
    let Some(header) = read_header(wav)? else {
        return Ok(None);
    };
    let file_len = wav.metadata().map_err(read_failed)?.len();
    let believable = header.data_start == format.data_start
        && header.data_len != 0
        && header.data_len != u64::from(u32::MAX)
        && header
            .data_start
            .checked_add(header.data_len)
            .is_some_and(|end| end <= file_len);
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

    /// Canonical 44-byte header; `data_len` 0 while recording.
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

    /// Tone: 440 Hz, 16-bit mono.
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

    /// MP3 frames as (kbit/s, rate, mono).
    fn frames(mp3: &[u8]) -> Vec<(u32, u32, bool)> {
        // MPEG-2 layer III.
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

    /// Test scratch dir.
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
        assert_eq!(bit_rate(7), 32_000);
    }

    #[test]
    fn the_longest_recording_is_what_the_limit_holds_at_the_bit_rate() {
        // Core's recommendation: (30 - 1) MiB * 3/4.
        let limit = (30 - 1) * 1024 * 1024 / 4 * 3;
        assert_eq!(longest_ms(limit, 32_000), 5_697_536, "94:57");
        assert_eq!(longest_ms(limit, 24_000), 7_596_714, "126:36");
        // 4000 bytes a second at 32 kbit/s.
        assert_eq!(longest_ms(HEADROOM_BYTES + 4000, 32_000), 1000);
        assert_eq!(longest_ms(0, 32_000), 0);
        // Tiny limit still ends.
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
        // Odd LIST chunk, padded to even.
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
        parse_header(b"OggS\0\0\0\0\0\0\0\0\0\0").unwrap_err();
        // Samples before fmt chunk.
        parse_header(b"RIFF\0\0\0\0WAVEdata\0\0\0\0").unwrap_err();
        // 12-bit: nothing writes it.
        parse_header(&header(1, 16_000, 12, 1, 0)).unwrap_err();
        // Compressed WAV, format 2 (ADPCM).
        parse_header(&header(1, 16_000, 16, 2, 0)).unwrap_err();
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

        // Stereo averaged; partial frame ignored.
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

            // Nothing yet, no error.
            transcoder.pump().unwrap();
            let head = header(1, SAMPLE_RATE, 16, 1, 0);
            append(&wav, &head[..20]);
            transcoder.pump().unwrap();
            append(&wav, &head[20..]);

            // Uneven pieces, one splitting a sample.
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
            // LAME delay: a few frames more.
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
        // 0.5 s at 32 kbit/s.
        let limit = HEADROOM_BYTES + 2000;
        let mut transcoder = Transcoder::new(wav.clone(), mp3.clone(), 32_000, limit).unwrap();
        assert_eq!(transcoder.limit_ms(), 500);

        // Three seconds recorded; first half kept.
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
        // On close: real length; tags after.
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
        // Nothing at asked path.
        transcoder.pump().unwrap();
        transcoder.follow(actual.clone());
        assert_eq!(transcoder.finish().unwrap(), Some(u64::from(SAMPLE_RATE)));
        assert!(mp3.exists() && !actual.exists() && !asked.exists());

        // Read from asked path: that is the recording.
        let mut transcoder = Transcoder::new(asked.clone(), mp3, 32_000, 0).unwrap();
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

        // Header, no sound.
        let transcoder = Transcoder::new(wav.clone(), mp3.clone(), 32_000, 0).unwrap();
        append(&wav, &header(1, SAMPLE_RATE, 16, 1, 0));
        assert_eq!(transcoder.finish().unwrap(), None);
        assert!(!mp3.exists() && !wav.exists());

        // Not a WAV.
        let transcoder = Transcoder::new(wav.clone(), mp3.clone(), 32_000, 0).unwrap();
        append(&wav, b"this is not a recording at all");
        transcoder.finish().unwrap_err();
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
