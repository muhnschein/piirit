//! Shrinks video to send. Phone video often exceeds `sys.msgsize_max_recommended`
//! (~22 MB), so some relays refuse it; core recodes pictures, not videos.
//! FFmpeg decodes and scales; x264 encodes at outgoing media quality's bit rate
//! (in-app camera's rates, `qml/pages/CapturePage.qml`), aimed under the
//! recommendation. deltachat-android does this with platform codecs
//! (`VideoRecoder.java`); Harbour app may not link those, so built in
//! (`build.rs`).
//!
//! Three steps, so shim can say what's coming first: [`probe`] reads header,
//! [`plan`] decides whether and at what size, [`recode`] does it.

// Shipped code: stricter lints than tests; see piirit-shim/src/lib.rs.
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

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;

/// Core's "balanced" (default).
pub const BALANCED: i32 = 0;
/// Core's "worse": slow or costly links.
pub const WORSE: i32 = 1;

/// Video in a file, as far as decodable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoCodec {
    /// No video.
    None,
    /// H.264: most phones record this.
    H264,
    /// HEVC: newer iPhones, some Android.
    Hevc,
    /// Undecodable; sent as is.
    Other,
}

/// Sound in a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioCodec {
    /// No sound.
    None,
    /// AAC; re-encoded at quality's rate.
    Aac,
    /// Other; copied as is.
    Other,
}

/// What a file holds, from header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    /// Play length, ms; 0 if unknown.
    pub duration_ms: u64,
    /// Picture as stored, before rotation.
    pub width: u32,
    /// See [`Self::width`].
    pub height: u32,
    /// Rotation, clockwise: 0, 90, 180 or 270.
    pub rotation: u32,
    /// Picture encoding.
    pub video_codec: VideoCodec,
    /// Picture bit/s; 0 if unknown.
    pub video_bit_rate: u64,
    /// Sound encoding.
    pub audio_codec: AudioCodec,
    /// Sound bit/s; 0 if unknown.
    pub audio_bit_rate: u64,
}

/// What to make of a video.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// Picture size as stored; even (4:2:0).
    pub width: u32,
    /// See [`Self::width`].
    pub height: u32,
    /// Picture bit/s.
    pub video_bit_rate: u64,
    /// Sound bit/s; 0 copies sound.
    pub audio_bit_rate: u64,
    /// Play length; with rates, sets output size.
    pub duration_ms: u64,
    /// Sound bit/s when copied.
    pub copied_audio_bit_rate: u64,
}

impl Target {
    /// Approx. new file size, bytes.
    #[must_use]
    pub fn predicted_bytes(&self) -> u64 {
        let audio = if self.audio_bit_rate > 0 {
            self.audio_bit_rate
        } else {
            self.copied_audio_bit_rate
        };
        self.video_bit_rate
            .saturating_add(audio)
            .saturating_mul(self.duration_ms)
            / 8000
    }
}

/// Outgoing quality: video and audio bit rates, long side, video floor.
struct Quality {
    video: u64,
    audio: u64,
    long_side: u32,
    least_video: u64,
}

/// Video bit/s per quality. Worse: `CapturePage.qml`'s worse rate (deltachat-ios
/// low preset) at phone screen width. Balanced: recoded only to fit
/// recommendation, so twice camera's balanced rate (1.5 Mbit/s smears 720p in
/// motion; near-limit video has room). Long ones squeezed below (see `plan`).
///
/// Least: floor for long video (480p at 700 kbit/s, 360p at 300 kbit/s still
/// show faces and hands). Below that: mush. Video that cannot fit at least is
/// sent at it, past recommendation; conversation flags it large, as any file.
const BALANCED_QUALITY: Quality = Quality {
    video: 3_000_000,
    audio: 64_000,
    long_side: 1280,
    least_video: 700_000,
};
const WORSE_QUALITY: Quality = Quality {
    video: 500_000,
    audio: 24_000,
    long_side: 640,
    least_video: 300_000,
};

/// Balanced recodes only when video is this close to recommended size (percent);
/// below, it fits and recoding only costs picture. Worse: any size.
const NEAR_LIMIT_PERCENT: u64 = 80;

/// Container cost over streams, as percent of budget held back: a few percent
/// for a long file's index.
const CONTAINER_SHARE: u64 = 5;

/// Recode or not, and at what size; `None` sends as is.
///
/// Balanced: only near recommended size (`NEAR_LIMIT_PERCENT`). Worse: any.
/// Always as is if not decodable, length unknown, or saving under a fifth (in-app
/// camera video is already at these rates).
///
/// `limit`: core's recommended max attachment; 0 if unknown. Output past it gets
/// lower bit rate, down to quality's least (below: not worth sending).
#[must_use]
pub fn plan(probe: &Probe, file_bytes: u64, media_quality: i32, limit: u64) -> Option<Target> {
    if !matches!(probe.video_codec, VideoCodec::H264 | VideoCodec::Hevc)
        || probe.duration_ms == 0
        || probe.width == 0
        || probe.height == 0
    {
        return None;
    }
    let quality = if media_quality == WORSE {
        &WORSE_QUALITY
    } else if limit > 0
        && file_bytes.saturating_mul(100) >= limit.saturating_mul(NEAR_LIMIT_PERCENT)
    {
        &BALANCED_QUALITY
    } else {
        return None;
    };
    let (audio_bit_rate, copied_audio_bit_rate) = match probe.audio_codec {
        AudioCodec::None => (0, 0),
        AudioCodec::Aac => (quality.audio, 0),
        AudioCodec::Other => (0, probe.audio_bit_rate),
    };
    let audio_cost = audio_bit_rate.max(copied_audio_bit_rate);

    let mut video_bit_rate = quality.video;
    if limit > 0 {
        // `duration_ms` non-zero: first check.
        let budget = limit
            .saturating_mul(8000)
            .checked_div(probe.duration_ms)
            .unwrap_or(0)
            .saturating_mul(100 - CONTAINER_SHARE)
            / 100;
        video_bit_rate =
            video_bit_rate.min(budget.saturating_sub(audio_cost).max(quality.least_video));
    }
    // Fewer bits, fewer pixels.
    let long_side = if video_bit_rate >= 1_000_000 {
        1280
    } else if video_bit_rate >= 450_000 {
        854
    } else {
        640
    }
    .min(quality.long_side);
    let (width, height) = fit(probe.width, probe.height, long_side);

    let target = Target {
        width,
        height,
        video_bit_rate,
        audio_bit_rate,
        duration_ms: probe.duration_ms,
        copied_audio_bit_rate,
    };
    // Under a fifth smaller: not worth the picture cost.
    (target.predicted_bytes().saturating_mul(5) < file_bytes.saturating_mul(4)).then_some(target)
}

/// `width` x `height` scaled to fit `long_side`; never up; sides even.
fn fit(width: u32, height: u32, long_side: u32) -> (u32, u32) {
    let long = width.max(height);
    let (width, height) = if long <= long_side {
        (width, height)
    } else {
        let scale = |side: u32| {
            // Integer rounding (sides a few thousand at most); `long` > `long_side`,
            // so not 0.
            let scaled = u64::from(side)
                .saturating_mul(u64::from(long_side))
                .saturating_add(u64::from(long) / 2)
                .checked_div(u64::from(long))
                .unwrap_or(0);
            u32::try_from(scaled).unwrap_or(long_side)
        };
        (scale(width), scale(height))
    };
    let even = |side: u32| (side & !1).max(2);
    (even(width), even(height))
}

/// How recoding ended, if not failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Written.
    Done,
    /// `progress` said stop; `output` partial.
    Cancelled,
}

/// Reads a file's header. Nothing decoded.
///
/// # Errors
///
/// When file can't be opened or isn't a container.
pub fn probe(path: &Path) -> Result<Probe, String> {
    let path = c_path(path)?;
    let mut raw = ffi::Probe::default();
    let mut error: [c_char; ERROR_LEN] = [0; ERROR_LEN];
    // SAFETY: `path` NUL-terminated, outlives call; `raw` matches recode.h;
    // error buffer's length passed beside it.
    #[allow(unsafe_code)]
    let result =
        unsafe { ffi::piirit_video_probe(path.as_ptr(), &mut raw, error.as_mut_ptr(), ERROR_LEN) };
    if result != ffi::OK {
        return Err(message(&error));
    }
    Ok(Probe {
        duration_ms: unsigned(raw.duration_ms),
        width: unsigned32(raw.width),
        height: unsigned32(raw.height),
        rotation: unsigned32(raw.rotation),
        video_codec: match raw.video_codec {
            0 => VideoCodec::None,
            1 => VideoCodec::H264,
            2 => VideoCodec::Hevc,
            _ => VideoCodec::Other,
        },
        video_bit_rate: unsigned(raw.video_bit_rate),
        audio_codec: match raw.audio_codec {
            0 => AudioCodec::None,
            1 => AudioCodec::Aac,
            _ => AudioCodec::Other,
        },
        audio_bit_rate: unsigned(raw.audio_bit_rate),
    })
}

/// Recodes `input` to `output` (MP4) as `target` says.
///
/// `progress` gets 0.0..=1.0, returns whether to go on; called on this thread.
/// Blocks (a minute of video is seconds of work): call from a worker.
///
/// # Errors
///
/// On read, decode or write failure. `output` then partial (as after cancel);
/// caller removes it.
pub fn recode<F>(
    input: &Path,
    output: &Path,
    target: &Target,
    progress: F,
) -> Result<Outcome, String>
where
    F: FnMut(f64) -> bool,
{
    let input = c_path(input)?;
    let output = c_path(output)?;
    let raw = ffi::Target {
        width: i32::try_from(target.width).map_err(|err| format!("too wide: {err}"))?,
        height: i32::try_from(target.height).map_err(|err| format!("too tall: {err}"))?,
        video_bit_rate: i64::try_from(target.video_bit_rate).unwrap_or(i64::MAX),
        audio_bit_rate: i64::try_from(target.audio_bit_rate).unwrap_or(i64::MAX),
    };
    let mut progress = progress;
    let mut error: [c_char; ERROR_LEN] = [0; ERROR_LEN];
    // SAFETY: paths NUL-terminated, outlive call; `raw` matches recode.h;
    // `context` is `progress`, alive for the call, only given to `report::<F>`.
    #[allow(unsafe_code)]
    let result = unsafe {
        ffi::piirit_video_recode(
            input.as_ptr(),
            output.as_ptr(),
            &raw,
            report::<F>,
            std::ptr::addr_of_mut!(progress).cast::<c_void>(),
            error.as_mut_ptr(),
            ERROR_LEN,
        )
    };
    match result {
        ffi::OK => Ok(Outcome::Done),
        ffi::CANCELLED => Ok(Outcome::Cancelled),
        _ => Err(message(&error)),
    }
}

/// Writes test clip: `seconds` of moving H.264 at `width` x `height`, `bit_rate`;
/// AAC tone if `with_sound`; `rotation` degrees clockwise. Tests only (no camera).
///
/// # Errors
///
/// When file can't be written.
#[doc(hidden)]
pub fn synth(
    path: &Path,
    (width, height): (u32, u32),
    seconds: u32,
    bit_rate: u64,
    rotation: u32,
    with_sound: bool,
) -> Result<(), String> {
    let path = c_path(path)?;
    let mut error: [c_char; ERROR_LEN] = [0; ERROR_LEN];
    let int =
        |value: u32| i32::try_from(value).map_err(|err| format!("{value} is too large: {err}"));
    let clip = ffi::Clip {
        width: int(width)?,
        height: int(height)?,
        seconds: int(seconds)?,
        rotation: int(rotation)?,
        bit_rate: i64::try_from(bit_rate).unwrap_or(i64::MAX),
        with_sound: i32::from(with_sound),
    };
    // SAFETY: as `probe`; `clip` matches recode.h; read-only.
    #[allow(unsafe_code)]
    let result =
        unsafe { ffi::piirit_video_synth(path.as_ptr(), &clip, error.as_mut_ptr(), ERROR_LEN) };
    if result == ffi::OK {
        Ok(())
    } else {
        Err(message(&error))
    }
}

/// Error buffer size for C messages.
const ERROR_LEN: usize = 256;

/// Called by C with progress. Closure says keep going; reply is its opposite.
extern "C" fn report<F: FnMut(f64) -> bool>(context: *mut c_void, permille: i32) -> c_int {
    // SAFETY: `context` is `recode`'s `&mut F`, alive and unborrowed during the call.
    #[allow(unsafe_code)]
    let progress = unsafe { &mut *context.cast::<F>() };
    // Unwinding through C is UB: a panic stops recoding instead.
    let keep_going = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        progress(f64::from(permille) / 1000.0)
    }))
    .unwrap_or(false);
    c_int::from(!keep_going)
}

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

fn message(error: &[c_char]) -> String {
    // SAFETY: C writes NUL-terminated string, or leaves buffer zeroed.
    #[allow(unsafe_code)]
    let text = unsafe { CStr::from_ptr(error.as_ptr()) };
    let text = text.to_string_lossy().into_owned();
    if text.is_empty() {
        "the video could not be recoded".into()
    } else {
        text
    }
}

fn unsigned(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn unsigned32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}

/// C side, as `csrc/recode.h` declares it.
mod ffi {
    use std::ffi::{c_char, c_int, c_void};

    pub const OK: c_int = 0;
    pub const CANCELLED: c_int = 1;

    #[repr(C)]
    #[derive(Default)]
    pub struct Probe {
        pub duration_ms: i64,
        pub width: i32,
        pub height: i32,
        pub rotation: i32,
        pub video_codec: i32,
        pub video_bit_rate: i64,
        pub audio_codec: i32,
        pub audio_bit_rate: i64,
    }

    #[repr(C)]
    pub struct Target {
        pub width: i32,
        pub height: i32,
        pub video_bit_rate: i64,
        pub audio_bit_rate: i64,
    }

    #[repr(C)]
    pub struct Clip {
        pub width: i32,
        pub height: i32,
        pub seconds: i32,
        pub rotation: i32,
        pub bit_rate: i64,
        pub with_sound: i32,
    }

    pub type Progress = extern "C" fn(context: *mut c_void, permille: i32) -> c_int;

    #[allow(unsafe_code)]
    extern "C" {
        pub fn piirit_video_probe(
            path: *const c_char,
            out: *mut Probe,
            error: *mut c_char,
            error_len: usize,
        ) -> c_int;

        pub fn piirit_video_recode(
            input: *const c_char,
            output: *const c_char,
            target: *const Target,
            progress: Progress,
            context: *mut c_void,
            error: *mut c_char,
            error_len: usize,
        ) -> c_int;

        pub fn piirit_video_synth(
            path: *const c_char,
            clip: *const Clip,
            error: *mut c_char,
            error_len: usize,
        ) -> c_int;
    }
}

#[cfg(test)]
mod tests {
    use super::{fit, plan, AudioCodec, Probe, Target, VideoCodec, BALANCED, WORSE};

    /// Minute of 1080p H.264 at 17 Mbit/s plus AAC, as phones record: ~128 MB.
    fn phone_minute() -> (Probe, u64) {
        let probe = Probe {
            duration_ms: 60_000,
            width: 1920,
            height: 1080,
            rotation: 90,
            video_codec: VideoCodec::H264,
            video_bit_rate: 17_000_000,
            audio_codec: AudioCodec::Aac,
            audio_bit_rate: 128_000,
        };
        (probe, (17_000_000 + 128_000) * 60 / 8)
    }

    const LIMIT: u64 = 22 * 1024 * 1024;

    #[test]
    fn a_phone_video_is_made_720p_at_the_balanced_rates() {
        let (mut probe, _) = phone_minute();
        probe.duration_ms = 30_000;
        let bytes = 17_128_000 / 8 * 30;
        let target = plan(&probe, bytes, BALANCED, LIMIT).unwrap();
        assert_eq!((target.width, target.height), (1280, 720));
        assert_eq!(target.video_bit_rate, 3_000_000);
        assert_eq!(target.audio_bit_rate, 64_000);
        // 3.064 Mbit/s for half a minute.
        assert_eq!(target.predicted_bytes(), 11_490_000);
    }

    #[test]
    fn a_minute_is_given_what_fits_under_the_recommendation() {
        let (probe, bytes) = phone_minute();
        let target = plan(&probe, bytes, BALANCED, LIMIT).unwrap();
        // 3 Mbit/s would land past 22 MiB; lower rate still enough for 720p.
        assert!(
            (2_500_000..3_000_000).contains(&target.video_bit_rate),
            "{target:?}"
        );
        assert!(target.predicted_bytes() <= LIMIT, "{target:?}");
        assert_eq!((target.width, target.height), (1280, 720));
    }

    #[test]
    fn worse_quality_is_smaller_and_cheaper() {
        let (probe, bytes) = phone_minute();
        let target = plan(&probe, bytes, WORSE, LIMIT).unwrap();
        assert_eq!((target.width, target.height), (640, 360));
        assert_eq!(target.video_bit_rate, 500_000);
        assert_eq!(target.audio_bit_rate, 24_000);
    }

    #[test]
    fn a_long_video_is_squeezed_under_the_recommendation() {
        let (mut probe, _) = phone_minute();
        probe.duration_ms = 3 * 60_000;
        let bytes = 17_128_000 / 8 * 180;
        let target = plan(&probe, bytes, BALANCED, LIMIT).unwrap();
        assert!(
            (700_000..1_000_000).contains(&target.video_bit_rate),
            "{target:?}"
        );
        assert!(target.predicted_bytes() <= LIMIT, "{target:?}");
        // Fewer bits buy fewer pixels.
        assert_eq!((target.width, target.height), (854, 480));
    }

    #[test]
    fn a_video_too_long_to_fit_is_not_squeezed_past_watchable() {
        let (mut probe, _) = phone_minute();
        probe.duration_ms = 10 * 60_000;
        let bytes = 17_128_000 / 8 * 600;
        let target = plan(&probe, bytes, BALANCED, LIMIT).unwrap();
        assert_eq!(target.video_bit_rate, 700_000);
        assert_eq!((target.width, target.height), (854, 480));
        assert!(target.predicted_bytes() > LIMIT);

        // Worse squeezes eight minutes ...
        probe.duration_ms = 8 * 60_000;
        let target = plan(&probe, bytes, WORSE, LIMIT).unwrap();
        assert!(
            (300_000..500_000).contains(&target.video_bit_rate),
            "{target:?}"
        );
        assert!(target.predicted_bytes() <= LIMIT, "{target:?}");
        // ... not an hour.
        probe.duration_ms = 60 * 60_000;
        let target = plan(&probe, bytes * 6, WORSE, LIMIT).unwrap();
        assert_eq!(target.video_bit_rate, 300_000);
        assert_eq!((target.width, target.height), (640, 360));
        assert!(target.predicted_bytes() > LIMIT);
    }

    #[test]
    fn at_balanced_quality_only_a_video_near_the_limit_is_made_smaller() {
        let (mut probe, _) = phone_minute();
        probe.duration_ms = 5_000;
        // 17 Mbit/s, 5 s: ~10.7 MB, half the limit.
        let bytes = 17_128_000 / 8 * 5;
        assert_eq!(plan(&probe, bytes, BALANCED, LIMIT), None);
        // Just under four fifths: as is ...
        assert_eq!(plan(&probe, LIMIT * 4 / 5 - 1, BALANCED, LIMIT), None);
        // ... above: smaller.
        let target = plan(&probe, LIMIT * 4 / 5 + 1, BALANCED, LIMIT).unwrap();
        assert_eq!(target.video_bit_rate, 3_000_000);
        // Worse: half-limit one still made smaller.
        assert!(plan(&probe, bytes, WORSE, LIMIT).is_some());
    }

    #[test]
    fn without_a_known_limit_only_worse_quality_recodes() {
        let (mut probe, _) = phone_minute();
        probe.duration_ms = 10 * 60_000;
        assert_eq!(plan(&probe, u64::MAX / 2, BALANCED, 0), None);
        let target = plan(&probe, u64::MAX / 2, WORSE, 0).unwrap();
        assert_eq!(target.video_bit_rate, 500_000);
    }

    #[test]
    fn a_video_already_at_these_rates_is_left_alone() {
        // Camera rates (1.5 Mbit/s, 64 kbit/s) at recommended size: rates decide.
        let probe = Probe {
            duration_ms: 30_000,
            width: 1280,
            height: 720,
            rotation: 0,
            video_codec: VideoCodec::H264,
            video_bit_rate: 1_500_000,
            audio_codec: AudioCodec::Aac,
            audio_bit_rate: 64_000,
        };
        let bytes = 1_564_000 * 30 / 8;
        assert_eq!(plan(&probe, bytes, BALANCED, bytes), None);
        // ... worse: a third of that, worth making.
        assert!(plan(&probe, bytes, WORSE, bytes).is_some());
    }

    #[test]
    fn what_cannot_be_decoded_or_measured_is_sent_as_it_is() {
        let (probe, bytes) = phone_minute();
        for codec in [VideoCodec::None, VideoCodec::Other] {
            let probe = Probe {
                video_codec: codec,
                ..probe
            };
            assert_eq!(plan(&probe, bytes, BALANCED, LIMIT), None, "{codec:?}");
        }
        let unmeasured = Probe {
            duration_ms: 0,
            ..probe
        };
        assert_eq!(plan(&unmeasured, bytes, BALANCED, LIMIT), None);
        let hevc = Probe {
            video_codec: VideoCodec::Hevc,
            ..probe
        };
        assert!(plan(&hevc, bytes, BALANCED, LIMIT).is_some());
    }

    #[test]
    fn sound_that_is_not_aac_is_copied_and_counted() {
        let (probe, _) = phone_minute();
        let probe = Probe {
            duration_ms: 30_000,
            audio_codec: AudioCodec::Other,
            audio_bit_rate: 256_000,
            ..probe
        };
        let bytes = 17_256_000 / 8 * 30;
        let target = plan(&probe, bytes, BALANCED, LIMIT).unwrap();
        assert_eq!(target.audio_bit_rate, 0, "copied, not encoded again");
        assert_eq!(target.predicted_bytes(), (3_000_000 + 256_000) * 30 / 8);

        let silent = Probe {
            audio_codec: AudioCodec::None,
            audio_bit_rate: 0,
            ..probe
        };
        let target = plan(&silent, bytes, BALANCED, LIMIT).unwrap();
        assert_eq!(target.predicted_bytes(), 3_000_000 * 30 / 8);
    }

    #[test]
    fn a_picture_is_scaled_to_fit_and_never_up() {
        assert_eq!(fit(1920, 1080, 1280), (1280, 720));
        // Portrait as stored (some phones write it).
        assert_eq!(fit(1080, 1920, 1280), (720, 1280));
        assert_eq!(fit(640, 480, 1280), (640, 480));
        // Odd sides made even; rounded, not cut.
        assert_eq!(fit(1919, 1081, 1280), (1280, 720));
        assert_eq!(fit(641, 361, 1280), (640, 360));
        assert_eq!(fit(4000, 3, 1280), (1280, 2));
    }

    #[test]
    fn the_predicted_size_counts_sound_either_way() {
        let target = Target {
            width: 2,
            height: 2,
            video_bit_rate: 800_000,
            audio_bit_rate: 0,
            duration_ms: 10_000,
            copied_audio_bit_rate: 0,
        };
        assert_eq!(target.predicted_bytes(), 1_000_000);
        let encoded = Target {
            audio_bit_rate: 80_000,
            ..target
        };
        assert_eq!(encoded.predicted_bytes(), 1_100_000);
    }
}
