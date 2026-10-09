//! A phone's video, made smaller: what comes out plays the same way,
//! for as long, and weighs what the plan said.
//!
//! The clips are made here (`piirit_video::synth`) the way a phone's
//! camera leaves one -- H.264 with B-frames, AAC, a turn in the display
//! matrix -- rather than carried as binary fixtures.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use piirit_video::{plan, probe, recode, synth, AudioCodec, Outcome, VideoCodec, BALANCED};

/// A directory of this test's own, gone again afterwards.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("piirit-video-{name}-{}", std::process::id()));
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

#[test]
fn a_clip_reads_back_as_it_was_made() {
    let scratch = Scratch::new("probe");
    let clip = scratch.file("clip.mp4");
    synth(&clip, (640, 360), 2, 4_000_000, 90, true).expect("make a clip");

    let probe = probe(&clip).expect("probe the clip");
    assert_eq!(probe.video_codec, VideoCodec::H264);
    assert_eq!((probe.width, probe.height), (640, 360));
    assert_eq!(probe.rotation, 90);
    assert_eq!(probe.audio_codec, AudioCodec::Aac);
    assert!((1900..=2200).contains(&probe.duration_ms), "{probe:?}");
    assert!(probe.video_bit_rate > 1_000_000, "{probe:?}");
}

#[test]
fn a_large_video_comes_out_smaller_and_plays_the_same_way() {
    let scratch = Scratch::new("recode");
    let clip = scratch.file("clip.mp4");
    let out = scratch.file("smaller.mp4");
    synth(&clip, (1280, 720), 3, 8_000_000, 270, true).expect("make a clip");
    let before = probe(&clip).expect("probe the clip");
    let bytes = std::fs::metadata(&clip).expect("measure the clip").len();

    let target = plan(&before, bytes, BALANCED, 0).expect("worth recoding");
    // Below the quality's own size, so the scaling is exercised too.
    let target = piirit_video::Target {
        width: 640,
        height: 360,
        ..target
    };
    let mut seen = Vec::new();
    let outcome = recode(&clip, &out, &target, |fraction| {
        seen.push(fraction);
        true
    })
    .expect("recode");
    assert_eq!(outcome, Outcome::Done);

    let after = probe(&out).expect("probe the result");
    assert_eq!(after.video_codec, VideoCodec::H264);
    assert_eq!((after.width, after.height), (640, 360));
    assert_eq!(after.rotation, 270, "the phone's turn is kept");
    assert_eq!(after.audio_codec, AudioCodec::Aac);
    assert!(
        after.duration_ms.abs_diff(before.duration_ms) < 200,
        "{before:?} -> {after:?}"
    );
    let smaller = std::fs::metadata(&out).expect("measure the result").len();
    assert!(smaller * 3 < bytes, "{bytes} -> {smaller}");
    // Within a quarter of what was planned.
    let planned = target.predicted_bytes();
    assert!(
        smaller.abs_diff(planned) * 4 < planned,
        "planned {planned}, made {smaller}"
    );

    // Progress only ever moves forward, and ends at the end.
    assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]), "{seen:?}");
    assert_eq!(seen.last().copied(), Some(1.0));
}

#[test]
fn a_cancel_stops_it() {
    let scratch = Scratch::new("cancel");
    let clip = scratch.file("clip.mp4");
    let out = scratch.file("smaller.mp4");
    synth(&clip, (640, 360), 3, 4_000_000, 0, false).expect("make a clip");
    let before = probe(&clip).expect("probe the clip");
    let target = plan(&before, u64::MAX / 2, BALANCED, 0).expect("worth recoding");

    let mut calls = 0;
    let outcome = recode(&clip, &out, &target, |fraction| {
        calls += 1;
        fraction < 0.3
    })
    .expect("recode");
    assert_eq!(outcome, Outcome::Cancelled);
    assert!(calls > 1, "asked more than once before it stopped");
}

#[test]
fn a_file_that_is_not_a_video_is_an_error_not_a_crash() {
    let scratch = Scratch::new("garbage");
    let junk = scratch.file("junk.mp4");
    std::fs::write(&junk, b"this is not a video at all").expect("write junk");
    assert!(probe(&junk).is_err());
    assert!(probe(&scratch.file("missing.mp4")).is_err());

    let target = piirit_video::Target {
        width: 640,
        height: 360,
        video_bit_rate: 500_000,
        audio_bit_rate: 0,
        duration_ms: 1000,
        copied_audio_bit_rate: 0,
    };
    let result = recode(&junk, &scratch.file("out.mp4"), &target, |_| true);
    assert!(result.is_err(), "{result:?}");
}
