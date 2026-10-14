//! The H.264 encoder video calls send through (`csrc/gmp_h264.cpp`), as
//! Gecko sees it: loaded from the directory build.rs lays out, handed
//! pictures by `gmp-check` (`tests/gmp_check.cpp`) the way Gecko's plugin
//! process hands them, and read back the way Gecko's packetizer reads
//! them. Each test is one scenario of the harness's; what it printed is
//! checked here.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The plugin's directory, `gmp-piirit/<version>`, as build.rs made it.
fn plugin_dir() -> PathBuf {
    PathBuf::from(env!("PIIRIT_GMP_PLUGIN"))
}

/// What `gmp-check` printed for a scenario, with the call log at `log`
/// when there is one.
fn run(scenario: &str, log: Option<&Path>) -> Vec<String> {
    let mut command = Command::new(env!("PIIRIT_GMP_CHECK"));
    command
        .arg(plugin_dir().join("libpiirit.so"))
        .arg(scenario)
        .env_remove("PIIRIT_CALL_LOG");
    if let Some(log) = log {
        command.env("PIIRIT_CALL_LOG", log);
    }
    let output = command.output().expect("run gmp-check");
    let text = String::from_utf8(output.stdout).expect("gmp-check prints text");
    assert!(output.status.success(), "{scenario}: {text}");
    text.lines().map(str::to_owned).collect()
}

/// One encoded picture, from a `frame ...` line.
#[derive(Debug)]
struct Frame {
    timestamp: u64,
    key: bool,
    bytes: u64,
    size: String,
    line: String,
    /// NAL unit types and sizes, in order.
    units: Vec<(u8, u32)>,
    sps: String,
}

fn field<'a>(line: &'a str, name: &str) -> &'a str {
    line.split(' ')
        .find_map(|word| {
            word.strip_prefix(name)
                .and_then(|rest| rest.strip_prefix('='))
        })
        .unwrap_or_else(|| panic!("no {name} in {line}"))
}

fn frames(lines: &[String]) -> Vec<Frame> {
    lines
        .iter()
        .filter(|line| line.starts_with("frame "))
        .map(|line| {
            let nals = field(line, "nals");
            assert_ne!(nals, "malformed", "Gecko could not read this: {line}");
            Frame {
                timestamp: field(line, "ts").parse().unwrap(),
                key: match field(line, "type") {
                    "key" => true,
                    "delta" => false,
                    other => panic!("frame type {other}: {line}"),
                },
                bytes: field(line, "bytes").parse().unwrap(),
                size: field(line, "size").to_owned(),
                line: line.clone(),
                units: nals
                    .split(',')
                    .map(|unit| {
                        let (kind, length) = unit.split_once(':').unwrap();
                        (kind.parse().unwrap(), length.parse().unwrap())
                    })
                    .collect(),
                sps: field(line, "sps").to_owned(),
            }
        })
        .collect()
}

/// Kilobits a second over `frames`, from their sizes and timestamps:
/// every picture but the last, over the time until the last.
fn kbps(frames: &[Frame]) -> u64 {
    let first = frames.first().unwrap().timestamp;
    let last = frames.last().unwrap().timestamp;
    let bits: u64 = frames[..frames.len() - 1].iter().map(|f| f.bytes * 8).sum();
    // Bits per microsecond, times a thousand, is kilobits a second.
    bits * 1000 / (last - first)
}

fn types(frame: &Frame) -> Vec<u8> {
    frame.units.iter().map(|&(kind, _)| kind).collect()
}

const FRAME_US: u64 = 33_333;
const SPS: u8 = 7;
const PPS: u8 = 8;
const IDR: u8 = 5;
const SLICE: u8 = 1;
const GMP_ENCODE_ERR: &str = "error 8";

#[test]
fn gecko_finds_an_h264_encoder_in_the_plugin_directory() {
    let dir = plugin_dir();
    // Gecko takes the plugin's name from the directory above the
    // version, less "gmp-", and loads lib<name>.so and <name>.info.
    assert_eq!(dir.parent().unwrap().file_name().unwrap(), "gmp-piirit");
    assert!(dir.join("libpiirit.so").is_file());
    let info = std::fs::read_to_string(dir.join("piirit.info")).unwrap();
    let fields: Vec<(&str, &str)> = info
        .lines()
        .map(|line| line.split_once(": ").expect("Field: value"))
        .collect();
    let names: Vec<&str> = fields.iter().map(|&(name, _)| name).collect();
    assert_eq!(names, ["Name", "Description", "Version", "APIs"]);
    // Encoding only: decoding H.264 is the phone's own decoder's.
    assert_eq!(fields[3].1, "encode-video[h264]");
}

#[test]
fn only_the_entry_points_are_there_to_find() {
    let lines = run("call", None);
    assert!(
        lines.contains(&"x264 symbols hidden".to_owned()),
        "{:?}",
        &lines[..3]
    );
    // 4 is GMPNotImplementedErr: no decoder, and no encoder without a
    // host to make its pictures with.
    assert!(lines.contains(&"decode-video 4".to_owned()), "{lines:?}");
    assert!(
        lines.contains(&"encode-video without host 4".to_owned()),
        "{lines:?}"
    );
}

#[test]
fn a_call_starts_on_a_keyframe_and_makes_another_when_asked() {
    let lines = run("call", None);
    let frames = frames(&lines);
    assert_eq!(
        frames.len(),
        60,
        "one picture out for every one in: {lines:?}"
    );
    assert!(
        lines.contains(&"inputs destroyed 60 of 60".to_owned()),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("error")),
        "{lines:?}"
    );
    for (i, frame) in frames.iter().enumerate() {
        // Each picture as soon as it went in, under its own timestamp.
        assert_eq!(
            frame.timestamp,
            1_000_000 + i as u64 * FRAME_US,
            "{}",
            frame.line
        );
        assert_eq!(frame.size, "320x240");
        // Four-byte sizes, a whole picture, and H.264 said in the info.
        assert_eq!(field(&frame.line, "buffer"), "4");
        assert_eq!(field(&frame.line, "complete"), "1");
        assert_eq!(field(&frame.line, "info"), "1/4");
        if i == 0 || i == 30 {
            // A receiver can start at either: SPS and PPS come with it.
            assert!(frame.key, "{}", frame.line);
            assert_eq!(types(frame)[..3], [SPS, PPS, IDR], "{}", frame.line);
            // Constrained Baseline (profile 66 with constraint_set1),
            // level 3.1: what Gecko offers.
            let sps = u32::from_str_radix(&frame.sps, 16).unwrap();
            assert_eq!(sps >> 16, 0x42, "{}", frame.line);
            assert_ne!(sps & 0x4000, 0, "{}", frame.line);
            assert_eq!(sps & 0xff, 31, "{}", frame.line);
        } else {
            assert!(!frame.key, "{}", frame.line);
            assert_eq!(types(frame), [SLICE], "{}", frame.line);
        }
    }
}

#[test]
fn the_bit_rate_follows_what_gecko_asks_for() {
    let frames = frames(&run("rates", None));
    assert_eq!(frames.len(), 120);
    // 1000 kbps for two seconds, then a tenth of that. The first
    // pictures fill the buffer, and the last carry what is left of it.
    let before = kbps(&frames[10..60]);
    let after = kbps(&frames[90..]);
    assert!((750..1250).contains(&before), "{before} kbps of 1000");
    assert!((75..125).contains(&after), "{after} kbps of 100");
}

#[test]
fn half_the_frame_rate_still_gets_the_bit_rate() {
    let frames = frames(&run("slow", None));
    assert_eq!(frames.len(), 90);
    // Set up for 30 pictures a second and given 15: each must carry
    // twice as much, or the call goes out at half its rate.
    let rate = kbps(&frames[30..]);
    assert!((480..720).contains(&rate), "{rate} kbps of 600");
}

#[test]
fn a_new_camera_size_starts_again_on_a_keyframe() {
    let frames = frames(&run("resize", None));
    assert_eq!(frames.len(), 20);
    assert!(frames[..10].iter().all(|f| f.size == "320x240"));
    assert!(frames[10..].iter().all(|f| f.size == "160x120"));
    assert!(frames[10].key, "{}", frames[10].line);
    assert_eq!(types(&frames[10])[..3], [SPS, PPS, IDR]);
    assert!(frames[11..].iter().all(|f| !f.key));
}

#[test]
fn an_odd_size_loses_its_last_column_and_row() {
    let frames = frames(&run("odd", None));
    assert_eq!(frames.len(), 5);
    assert!(frames.iter().all(|f| f.size == "320x240"), "{frames:?}");
}

#[test]
fn packetization_mode_0_keeps_every_unit_to_one_packet() {
    let frames = frames(&run("slices", None));
    assert_eq!(frames.len(), 10);
    for frame in &frames {
        assert!(frame.units.len() > 2, "{}", frame.line);
        for &(kind, length) in &frame.units {
            assert!(length <= 400, "{kind}:{length} in {}", frame.line);
        }
    }
}

#[test]
fn an_encoder_that_cannot_start_says_so_and_takes_every_picture_back() {
    let lines = run("broken", None);
    assert!(frames(&lines).is_empty(), "{lines:?}");
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("error"))
            .count(),
        1,
        "Gecko is told once: {lines:?}"
    );
    assert!(lines.contains(&GMP_ENCODE_ERR.to_owned()), "{lines:?}");
    assert!(
        lines.contains(&"inputs destroyed 3 of 3".to_owned()),
        "{lines:?}"
    );
}

/// A call log, as the call host leaves one: made, and empty.
struct Log(PathBuf);

impl Log {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("piirit-gmp-{name}-{}.log", std::process::id()));
        std::fs::write(&path, "").unwrap();
        Self(path)
    }

    fn lines(&self) -> Vec<String> {
        std::fs::read_to_string(&self.0)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Log {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn what_encoding_costs_goes_in_the_call_log() {
    let log = Log::new("long");
    run("long", Some(&log.0));
    let lines = log.lines();
    assert_eq!(
        lines[0], "encoder x264 320x240, up to 30 fps, 300 kbps, constrained baseline",
        "{lines:?}"
    );
    assert!(lines[1].starts_with("encoder first picture: "), "{lines:?}");
    assert!(lines[1].ends_with(" bytes, key"), "{lines:?}");
    // Every thirty seconds of the call, and once at its end.
    let summaries: Vec<&String> = lines[2..].iter().collect();
    assert_eq!(summaries.len(), 3, "{lines:?}");
    assert!(
        summaries[0].starts_with("encoder so far: 31 frames (1 key) in 30 s, "),
        "{lines:?}"
    );
    assert!(
        summaries[1].starts_with("encoder so far: 61 frames (1 key) in 60 s, "),
        "{lines:?}"
    );
    assert!(
        summaries[2].starts_with("encoder stopped: 62 frames (1 key) in 61 s, "),
        "{lines:?}"
    );
    for summary in summaries {
        assert!(summary.contains(" ms CPU each, "), "{summary}");
        assert!(summary.ends_with(" of 300, 320x240"), "{summary}");
    }
}

#[test]
fn an_encoder_that_cannot_start_says_why_in_the_call_log() {
    let log = Log::new("broken");
    run("broken", Some(&log.0));
    assert_eq!(log.lines(), ["encoder cannot start 0x0 at 300 kbps"]);
}

#[test]
fn no_call_log_is_made_where_the_call_host_made_none() {
    let path = std::env::temp_dir().join(format!("piirit-gmp-none-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&path);
    run("odd", Some(&path));
    assert!(!path.exists());
}
