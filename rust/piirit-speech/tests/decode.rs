//! Voice files as other clients send them come out as 16 kHz mono, as long as they play.
//!
//! WAV and Ogg Opus written here; AAC (M4A, ADTS) made by the app's own FFmpeg
//! (`piirit_video::synth_tone`); MP3 is whisper.cpp's sample. No binary fixture added.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use piirit_speech::{decode, Error, SAMPLE_RATE};

/// Test's own directory, removed after.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("piirit-speech-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("make the scratch directory");
        Self(dir)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sample(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/whisper.cpp/samples")
        .join(name)
}

/// 16-bit PCM WAV of `seconds` of `hz` tone, `rate`, `channels`.
fn write_wav(path: &Path, seconds: u32, hz: f64, rate: u32, channels: u16) {
    let frames = seconds * rate;
    let data_len = frames * u32::from(channels) * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for frame in 0..frames {
        // Within 0.3 of full scale: fits.
        #[allow(clippy::cast_possible_truncation)]
        let value = (0.3
            * (2.0 * std::f64::consts::PI * hz * f64::from(frame) / f64::from(rate)).sin()
            * 32767.0) as i16;
        for _ in 0..channels {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    std::fs::write(path, bytes).expect("write the WAV");
}

/// Tone's pitch, from upward zero crossings over the middle of the sound (codec
/// start-up and tail left out).
fn pitch(samples: &[f32]) -> f64 {
    let middle = &samples[samples.len() / 4..samples.len() * 3 / 4];
    let crossings = middle
        .windows(2)
        .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
        .count();
    count(crossings) * f64::from(SAMPLE_RATE) / count(middle.len())
}

/// Test clips are short: any count fits.
fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).expect("short clip"))
}

fn seconds_of(samples: &[f32]) -> f64 {
    count(samples.len()) / f64::from(SAMPLE_RATE)
}

fn assert_seconds(samples: &[f32], seconds: f64, slack: f64) {
    let got = seconds_of(samples);
    assert!(
        (got - seconds).abs() <= slack,
        "{got} s of sound, wanted {seconds} s"
    );
}

#[test]
fn a_wav_comes_out_as_sixteen_kilohertz_mono() {
    let scratch = Scratch::new("wav");
    let wav = scratch.file("voice.wav");
    write_wav(&wav, 2, 440.0, 44_100, 2);

    let samples = decode(&wav, 60).expect("decode the WAV");
    // Every sample, resampler's tail too.
    assert_eq!(samples.len(), 2 * SAMPLE_RATE as usize);
    let pitch = pitch(&samples);
    assert!((pitch - 440.0).abs() < 5.0, "{pitch} Hz");
    // Both channels, same: mixed, not summed past full scale.
    let loudest = samples.iter().fold(0.0f32, |most, s| most.max(s.abs()));
    assert!((0.25..0.35).contains(&loudest), "peak {loudest}");
}

#[test]
fn an_iphone_m4a_and_an_android_adts_stream_decode() {
    let scratch = Scratch::new("aac");
    for (name, rate, channels, adts) in [
        ("voice.m4a", 44_100, 1, false),
        ("voice.aac", 48_000, 1, true),
        ("stereo.m4a", 48_000, 2, false),
    ] {
        let file = scratch.file(name);
        piirit_video::synth_tone(&file, 3, rate, channels, adts).expect("make the sound");
        let samples = decode(&file, 60).unwrap_or_else(|err| panic!("{name}: {err}"));
        // ADTS has no edit list: encoder's start-up frame stays.
        assert_seconds(&samples, 3.0, 0.1);
        let pitch = pitch(&samples);
        assert!((pitch - 440.0).abs() < 5.0, "{name}: {pitch} Hz");
    }
}

#[test]
fn a_stream_that_changes_rate_and_channels_midway_decodes_whole() {
    let scratch = Scratch::new("change");
    let (first, second) = (scratch.file("first.aac"), scratch.file("second.aac"));
    piirit_video::synth_tone(&first, 2, 48_000, 1, true).expect("make the sound");
    piirit_video::synth_tone(&second, 2, 22_050, 2, true).expect("make the sound");
    // ADTS: each frame carries its own header, so files join as one stream.
    let joined = scratch.file("joined.aac");
    let mut bytes = std::fs::read(&first).unwrap();
    bytes.extend(std::fs::read(&second).unwrap());
    std::fs::write(&joined, bytes).unwrap();

    let samples = decode(&joined, 60).expect("decode the joined stream");
    assert_seconds(&samples, 4.0, 0.15);
    let pitch = pitch(&samples);
    assert!((pitch - 440.0).abs() < 5.0, "{pitch} Hz");
}

#[test]
fn an_mp3_reads_as_long_as_the_same_speech_in_wav() {
    let wav = decode(&sample("jfk.wav"), 60).expect("decode the WAV");
    let mp3 = decode(&sample("jfk.mp3"), 60).expect("decode the MP3");
    assert_seconds(&wav, 11.0, 0.01);
    // MP3 frames pad start and end by up to ~50 ms.
    assert_seconds(&mp3, seconds_of(&wav), 0.1);
    let energy = |samples: &[f32]| {
        samples.iter().map(|s| f64::from(s * s)).sum::<f64>() / count(samples.len())
    };
    let (wav_energy, mp3_energy) = (energy(&wav), energy(&mp3));
    assert!(
        (mp3_energy / wav_energy - 1.0).abs() < 0.1,
        "loudness {mp3_energy} vs {wav_energy}"
    );
}

/// Ogg page CRC: polynomial 0x04c11db7, not reflected, zero start.
fn ogg_crc(bytes: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &byte in bytes {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn ogg_page(out: &mut Vec<u8>, kind: u8, granule: u64, sequence: u32, packet: &[u8]) {
    assert!(packet.len() < 255);
    let start = out.len();
    out.extend_from_slice(b"OggS");
    out.push(0);
    out.push(kind);
    out.extend_from_slice(&granule.to_le_bytes());
    out.extend_from_slice(&0x5049_4952u32.to_le_bytes());
    out.extend_from_slice(&sequence.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.push(1);
    out.push(u8::try_from(packet.len()).expect("packet in one segment"));
    out.extend_from_slice(packet);
    let crc = ogg_crc(&out[start..]);
    out[start + 22..start + 26].copy_from_slice(&crc.to_le_bytes());
}

/// Ogg Opus of `packets` 20 ms CELT frames carrying no audio data (decoded as silence):
/// container and decoder path, not sound. No Opus encoder in the app's FFmpeg.
fn write_ogg_opus(path: &Path, packets: u32) {
    let mut head = b"OpusHead".to_vec();
    head.push(1);
    head.push(1);
    head.extend_from_slice(&312u16.to_le_bytes());
    head.extend_from_slice(&48_000u32.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&5u32.to_le_bytes());
    tags.extend_from_slice(b"piiri");
    tags.extend_from_slice(&0u32.to_le_bytes());

    let mut out = Vec::new();
    ogg_page(&mut out, 2, 0, 0, &head);
    ogg_page(&mut out, 0, 0, 1, &tags);
    for packet in 0..packets {
        let granule = u64::from(packet + 1) * 960;
        let last = packet + 1 == packets;
        // TOC: config 31 (CELT fullband 20 ms), mono, one frame; then frame bytes all
        // zero: silence flag set, rest ignored.
        ogg_page(
            &mut out,
            if last { 4 } else { 0 },
            granule,
            packet + 2,
            &[0xf8, 0x80, 0, 0],
        );
    }
    std::fs::write(path, out).expect("write the Ogg Opus");
}

#[test]
fn an_ogg_opus_file_decodes_to_its_length() {
    let scratch = Scratch::new("opus");
    let opus = scratch.file("voice.ogg");
    write_ogg_opus(&opus, 100);
    let samples = decode(&opus, 60).expect("decode the Ogg Opus");
    // 2 s less 312 samples (6.5 ms) pre-skip.
    assert_seconds(&samples, 2.0, 0.02);
}

#[test]
fn decoding_stops_at_the_cap() {
    let samples = decode(&sample("jfk.wav"), 3).expect("decode the WAV");
    assert_eq!(samples.len(), 3 * SAMPLE_RATE as usize);
    let none = decode(&sample("jfk.mp3"), 0).expect("decode nothing");
    assert!(none.is_empty());
}

#[test]
fn what_is_not_sound_is_an_audio_error() {
    let scratch = Scratch::new("not-sound");
    let text = scratch.file("voice.mp3");
    std::fs::write(&text, "not a voice message\n".repeat(100)).unwrap();
    assert!(
        matches!(decode(&text, 60), Err(Error::Audio(_))),
        "text file"
    );

    assert!(
        matches!(
            decode(&scratch.file("missing.m4a"), 60),
            Err(Error::Audio(_))
        ),
        "missing file"
    );

    let silent_video = scratch.file("clip.mp4");
    piirit_video::synth(&silent_video, (64, 64), 1, 100_000, 0, false).unwrap();
    match decode(&silent_video, 60) {
        Err(Error::Audio(why)) => assert!(why.contains("no sound"), "{why}"),
        other => panic!("video without sound: {other:?}"),
    }
}
