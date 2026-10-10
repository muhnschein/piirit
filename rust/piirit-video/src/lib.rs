//! Make a video small enough to send.
//!
//! A video picked from the gallery or shared into Piirit is whatever the
//! phone recorded, which is commonly well past what the core recommends
//! (`sys.msgsize_max_recommended`, about 22 MB) and so past what some
//! relays take. The core recodes pictures and leaves videos alone, so
//! this recodes them first: decoded by FFmpeg, scaled, encoded again by
//! x264 at the bit rate the outgoing media quality setting chooses -- the
//! rates the in-app camera records at (`qml/pages/CapturePage.qml`) --
//! and aimed under the recommendation. deltachat-android does the same
//! with the platform's codecs (`VideoRecoder.java`); a Harbour app may
//! not link the platform's, so these are built in (`build.rs`).
//!
//! Three steps, so the shim can say what is about to happen before it
//! happens: [`probe`] reads a file's header, [`plan`] decides from that
//! whether recoding is worth it and at what size, and [`recode`] does it.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;

/// The outgoing media quality the core calls "balanced": its own
/// default.
pub const BALANCED: i32 = 0;
/// The one it calls "worse quality", for slow or expensive connections.
pub const WORSE: i32 = 1;

/// The video in a file, as far as this can decode it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoCodec {
    /// No video at all.
    None,
    /// H.264, what most phones record.
    H264,
    /// HEVC, what newer iPhones and some Android phones record.
    Hevc,
    /// Something this cannot decode, which is then sent as it is.
    Other,
}

/// The sound in a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioCodec {
    /// No sound.
    None,
    /// AAC, which is encoded again at the quality's rate.
    Aac,
    /// Anything else, which is copied as it is.
    Other,
}

/// What a file holds, from its header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    /// How long it plays, in milliseconds; 0 when the file does not say.
    pub duration_ms: u64,
    /// The picture as stored, before the phone's turn is applied.
    pub width: u32,
    /// See [`Self::width`].
    pub height: u32,
    /// The phone's turn, clockwise: 0, 90, 180 or 270.
    pub rotation: u32,
    /// What the picture is encoded as.
    pub video_codec: VideoCodec,
    /// What the picture costs, in bits a second; 0 when unknown.
    pub video_bit_rate: u64,
    /// What the sound is encoded as.
    pub audio_codec: AudioCodec,
    /// What the sound costs, in bits a second; 0 when unknown.
    pub audio_bit_rate: u64,
}

/// What to make of a video.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// The picture's size as stored; even, as 4:2:0 needs.
    pub width: u32,
    /// See [`Self::width`].
    pub height: u32,
    /// The picture's bit rate.
    pub video_bit_rate: u64,
    /// The sound's bit rate; 0 copies the sound as it is.
    pub audio_bit_rate: u64,
    /// How long it plays, which with the rates is how large it comes out.
    pub duration_ms: u64,
    /// What the sound costs when it is copied rather than encoded again.
    pub copied_audio_bit_rate: u64,
}

impl Target {
    /// About how large the new file will be, in bytes.
    #[must_use]
    pub fn predicted_bytes(&self) -> u64 {
        let audio = if self.audio_bit_rate > 0 {
            self.audio_bit_rate
        } else {
            self.copied_audio_bit_rate
        };
        (self.video_bit_rate + audio).saturating_mul(self.duration_ms) / 8000
    }
}

/// One outgoing media quality: what a second of video may cost, and how
/// large a picture that buys.
struct Quality {
    video: u64,
    audio: u64,
    long_side: u32,
}

/// The rates `CapturePage.qml` records at for each quality, which are
/// deltachat-android's ceiling for a recoded video and roughly what
/// deltachat-ios's low preset comes to. The picture sizes are what x264
/// spends those rates on well: 720p at 1.5 Mbit/s, and a phone screen's
/// width at 500 kbit/s.
const BALANCED_QUALITY: Quality = Quality {
    video: 1_500_000,
    audio: 64_000,
    long_side: 1280,
};
const WORSE_QUALITY: Quality = Quality {
    video: 500_000,
    audio: 24_000,
    long_side: 640,
};

/// The least a second of picture is given when a long video is squeezed
/// under the recommended size. Below it the picture is mush; a video that
/// cannot fit above it is sent at it, and the conversation says it is
/// large, as it says of any file.
const LEAST_VIDEO: u64 = 150_000;

/// How close to the recommended size, in percent of it, a video has to
/// come before balanced quality makes it smaller. Below it the video is
/// sent as it was recorded: it fits, and recoding would only cost
/// picture. Worse quality asks for small files, so it recodes whatever
/// size the video is.
const NEAR_LIMIT_PERCENT: u64 = 80;

/// What the container costs on top of the streams, as a share of the
/// budget held back: a few percent for the index of a long file.
const CONTAINER_SHARE: u64 = 5;

/// Whether to recode a file and at what size, or `None` to send it as it
/// is.
///
/// At balanced quality only a video that comes near the recommended size
/// (`NEAR_LIMIT_PERCENT`) is recoded; at worse quality any video is.
/// Either way it is sent as it is when it is not a video this can
/// decode, when it does not say how long it is, or when recoding would
/// not save at least a fifth of it -- a video the in-app camera recorded
/// is already at these rates, and recoding one again would only cost
/// picture.
///
/// `limit` is the core's recommended largest attachment, 0 while unknown.
/// A video that would come out past it is given a lower bit rate to fit,
/// down to a floor below which the picture is not worth sending.
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
        let budget = limit.saturating_mul(8000) / probe.duration_ms * (100 - CONTAINER_SHARE) / 100;
        video_bit_rate = video_bit_rate.min(budget.saturating_sub(audio_cost).max(LEAST_VIDEO));
    }
    // Fewer pixels for fewer bits, so each one is worth having.
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
    // At least a fifth smaller, or not worth the picture it costs.
    (target.predicted_bytes().saturating_mul(5) < file_bytes.saturating_mul(4)).then_some(target)
}

/// `width` by `height` scaled to fit `long_side`, never up, each side
/// even.
fn fit(width: u32, height: u32, long_side: u32) -> (u32, u32) {
    let long = width.max(height);
    let (width, height) = if long <= long_side {
        (width, height)
    } else {
        let scale = |side: u32| {
            // Rounded, in integers: the sides are a few thousand at most.
            let scaled =
                (u64::from(side) * u64::from(long_side) + u64::from(long) / 2) / u64::from(long);
            u32::try_from(scaled).unwrap_or(long_side)
        };
        (scale(width), scale(height))
    };
    let even = |side: u32| (side & !1).max(2);
    (even(width), even(height))
}

/// How a recoding ended, when it did not fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The new file is written.
    Done,
    /// Stopped because `progress` said so; what was written is partial.
    Cancelled,
}

/// Read what a file holds from its header. Cheap: nothing is decoded.
///
/// # Errors
///
/// When the file cannot be opened, or is not a container this reads.
pub fn probe(path: &Path) -> Result<Probe, String> {
    let path = c_path(path)?;
    let mut raw = ffi::Probe::default();
    let mut error = [0 as c_char; ERROR_LEN];
    // SAFETY: `path` is a NUL-terminated string that outlives the call,
    // `raw` is a struct of the layout recode.h declares, and the error
    // buffer's length is passed beside it.
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

/// Recode `input` into `output`, an MP4, as `target` says.
///
/// `progress` hears how far along it is, from 0.0 to 1.0, and answers
/// whether to keep going. It is called on this thread, so this is a call
/// for a worker thread: a minute of phone video is some seconds of work.
///
/// # Errors
///
/// When the file cannot be read, decoded or written. What was written
/// of `output` is then partial, as it is after a cancel, and the caller
/// removes it.
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
        width: i32::try_from(target.width).map_err(|_| "too wide".to_string())?,
        height: i32::try_from(target.height).map_err(|_| "too tall".to_string())?,
        video_bit_rate: i64::try_from(target.video_bit_rate).unwrap_or(i64::MAX),
        audio_bit_rate: i64::try_from(target.audio_bit_rate).unwrap_or(i64::MAX),
    };
    let mut progress = progress;
    let mut error = [0 as c_char; ERROR_LEN];
    // SAFETY: both paths are NUL-terminated and outlive the call; `raw`
    // has recode.h's layout; `context` points at `progress`, which lives
    // on this frame for the whole call, and is only ever handed back to
    // `report::<F>`, the one function that knows its type.
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

/// Write a test clip: `seconds` of moving H.264 at `width` by `height`
/// and `bit_rate`, with a tone in AAC when `with_sound`, turned by
/// `rotation` degrees clockwise. For tests, which have no camera.
///
/// # Errors
///
/// When the file cannot be written.
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
    let mut error = [0 as c_char; ERROR_LEN];
    let int = |value: u32| i32::try_from(value).map_err(|_| format!("{value} is too large"));
    let clip = ffi::Clip {
        width: int(width)?,
        height: int(height)?,
        seconds: int(seconds)?,
        rotation: int(rotation)?,
        bit_rate: i64::try_from(bit_rate).unwrap_or(i64::MAX),
        with_sound: i32::from(with_sound),
    };
    // SAFETY: as for `probe`; `clip` is the layout recode.h declares,
    // and is only read.
    #[allow(unsafe_code)]
    let result =
        unsafe { ffi::piirit_video_synth(path.as_ptr(), &clip, error.as_mut_ptr(), ERROR_LEN) };
    if result == ffi::OK {
        Ok(())
    } else {
        Err(message(&error))
    }
}

/// Room for what the C side says went wrong.
const ERROR_LEN: usize = 256;

/// Called by the C side with the progress, and answering whether to
/// stop: the closure says whether to keep going, so the answer is its
/// opposite.
extern "C" fn report<F: FnMut(f64) -> bool>(context: *mut c_void, permille: i32) -> c_int {
    // SAFETY: `context` is the `&mut F` that `recode` passed, which is
    // alive and not otherwise borrowed for the length of that call.
    #[allow(unsafe_code)]
    let progress = unsafe { &mut *context.cast::<F>() };
    // Unwinding through C is undefined, so a panic stops the recoding
    // instead.
    let keep_going = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        progress(f64::from(permille) / 1000.0)
    }))
    .unwrap_or(false);
    c_int::from(!keep_going)
}

fn c_path(path: &Path) -> Result<CString, String> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("{} has a NUL in it", path.display()))
}

fn message(error: &[c_char]) -> String {
    // SAFETY: the C side writes a NUL-terminated string into the buffer
    // or leaves it zeroed, which is the empty string.
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

/// The C side, as `csrc/recode.h` declares it.
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

    /// A minute of 1080p H.264 at 17 Mbit/s with AAC, as a phone records
    /// it: about 128 MB.
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
        let (probe, bytes) = phone_minute();
        let target = plan(&probe, bytes, BALANCED, LIMIT).unwrap();
        assert_eq!((target.width, target.height), (1280, 720));
        assert_eq!(target.video_bit_rate, 1_500_000);
        assert_eq!(target.audio_bit_rate, 64_000);
        // 1.564 Mbit/s for a minute.
        assert_eq!(target.predicted_bytes(), 11_730_000);
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
        probe.duration_ms = 10 * 60_000;
        let bytes = 17_128_000 / 8 * 600;
        let target = plan(&probe, bytes, BALANCED, LIMIT).unwrap();
        assert!(target.video_bit_rate < 1_500_000, "{target:?}");
        assert!(target.predicted_bytes() <= LIMIT, "{target:?}");
        // Fewer bits buy fewer pixels.
        assert_eq!((target.width, target.height), (640, 360));
    }

    #[test]
    fn a_video_too_long_to_fit_still_gets_a_watchable_rate() {
        let (mut probe, _) = phone_minute();
        probe.duration_ms = 60 * 60_000;
        let target = plan(&probe, u64::MAX / 2, BALANCED, LIMIT).unwrap();
        assert_eq!(target.video_bit_rate, 150_000);
        assert!(target.predicted_bytes() > LIMIT);
    }

    #[test]
    fn at_balanced_quality_only_a_video_near_the_limit_is_made_smaller() {
        let (mut probe, _) = phone_minute();
        probe.duration_ms = 5_000;
        // 17 Mbit/s for five seconds: about 10.7 MB, half the limit.
        let bytes = 17_128_000 / 8 * 5;
        assert_eq!(plan(&probe, bytes, BALANCED, LIMIT), None);
        // Just under four fifths of it is still sent as it is ...
        assert_eq!(plan(&probe, LIMIT * 4 / 5 - 1, BALANCED, LIMIT), None);
        // ... and from there on it is made smaller.
        let target = plan(&probe, LIMIT * 4 / 5 + 1, BALANCED, LIMIT).unwrap();
        assert_eq!(target.video_bit_rate, 1_500_000);
        // At worse quality the half-limit one is made smaller too.
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
        // What the in-app camera records: 1.5 Mbit/s and 64 kbit/s.
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
        assert_eq!(plan(&probe, 1_564_000 * 30 / 8, BALANCED, LIMIT), None);
        // ... but at worse quality, a third of that is worth making.
        assert!(plan(&probe, 1_564_000 * 30 / 8, WORSE, LIMIT).is_some());
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
        let (probe, bytes) = phone_minute();
        let probe = Probe {
            audio_codec: AudioCodec::Other,
            audio_bit_rate: 256_000,
            ..probe
        };
        let target = plan(&probe, bytes, BALANCED, LIMIT).unwrap();
        assert_eq!(target.audio_bit_rate, 0, "copied, not encoded again");
        assert_eq!(target.predicted_bytes(), (1_500_000 + 256_000) * 60 / 8);

        let silent = Probe {
            audio_codec: AudioCodec::None,
            audio_bit_rate: 0,
            ..probe
        };
        let target = plan(&silent, bytes, BALANCED, LIMIT).unwrap();
        assert_eq!(target.predicted_bytes(), 1_500_000 * 60 / 8);
    }

    #[test]
    fn a_picture_is_scaled_to_fit_and_never_up() {
        assert_eq!(fit(1920, 1080, 1280), (1280, 720));
        // Portrait as stored, which some phones write.
        assert_eq!(fit(1080, 1920, 1280), (720, 1280));
        assert_eq!(fit(640, 480, 1280), (640, 480));
        // Odd sides are made even, and rounded rather than cut.
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
