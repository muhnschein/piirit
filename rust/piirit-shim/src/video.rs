//! A picked video, made small enough to send before it is sent.
//!
//! The core recodes a picture on its way out and sends a video as it is
//! (`media.rs`), so a video from the gallery goes out at whatever the
//! phone recorded it at -- commonly several times what the core
//! recommends, and so past what some relays take (issue #111). This asks
//! `piirit-video` what the file holds as it is picked, plans a smaller
//! one at the outgoing media quality's rates, and makes it on a thread of
//! its own when the reader sends.
//!
//! What is made goes in the app's cache, beside the captures, and is
//! gone once the core has copied it into its blob directory. The reader
//! sees the file named as they picked it, with the suffix of what it now
//! is.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use piirit_video::{Outcome, Target};

use crate::qr::cache_file;

/// The directory under the cache a recoded video waits in.
const RECODED_DIR: &str = "recoded";

/// The suffixes of the files FFmpeg is built to read here: MP4 and
/// QuickTime, which are what phones record. A video in anything else is
/// sent as it is.
const VIDEOS: [&str; 5] = ["mp4", "m4v", "mov", "3gp", "3g2"];

/// Whether `path` is a video this can read, by its suffix -- which is
/// all a picker says about a file, and enough to not open every
/// document to look.
pub(crate) fn is_video(path: &str) -> bool {
    let suffix = Path::new(path)
        .extension()
        .map(|suffix| suffix.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    VIDEOS.contains(&suffix.as_str())
}

/// How to make the video at `path` smaller, or `None` to send it as it
/// is: not a video this reads, already small, or not readable at all --
/// which the core is then left to make of it, as before.
///
/// Reads only the file's header, so it is cheap enough to ask as a file
/// is picked.
pub(crate) fn plan(path: &str, media_quality: u32, limit: u64) -> Option<Target> {
    if !is_video(path) {
        return None;
    }
    let probe = piirit_video::probe(Path::new(path)).ok()?;
    let bytes = std::fs::metadata(path).ok()?.len();
    piirit_video::plan(
        &probe,
        bytes,
        i32::try_from(media_quality).unwrap_or(0),
        limit,
    )
}

/// What the recipient sees the recoded file called: the picked file's
/// name with the suffix of what it now is.
pub(crate) fn recoded_name(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "video".into());
    format!("{stem}.mp4")
}

/// Numbers the recoded files, with the process: two chats can each be
/// making one.
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A fresh path to write a recoded video to, in the cache.
fn recoded_path() -> Result<PathBuf, String> {
    let dir = cache_file(&format!("{RECODED_DIR}/.keep"))?
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "no cache directory".to_string())?;
    let number = NEXT.fetch_add(1, Ordering::Relaxed);
    Ok(dir.join(format!("video-{}-{number}.mp4", std::process::id())))
}

/// Remove a recoded video once nothing needs it. Bounded to the recoded
/// directory, so it cannot take a picked file with it whatever it is
/// handed.
pub(crate) fn discard(path: &str) {
    let Ok(dir) = cache_file(&format!("{RECODED_DIR}/.keep")) else {
        return;
    };
    let path = Path::new(path);
    if path.parent() == dir.parent() && path.file_name().is_some() {
        let _ = std::fs::remove_file(path);
    }
}

/// How a recoding came out, for the conversation to act on.
pub(crate) enum Prepared {
    /// Here is the smaller file.
    Made(String),
    /// The reader stopped it; nothing is sent.
    Cancelled,
    /// It could not be made, and why; the original is sent instead.
    Failed(String),
}

/// Start making `input` smaller on a thread of its own.
///
/// `progress` hears how far along it is, as whole percents only so the
/// interface is not woken for every frame; `done` hears how it came
/// out. Both are called on that thread, so both should be
/// `queued_callback`s. `cancel` stops it.
///
/// # Errors
///
/// When no thread could be started, in which case neither is called.
pub(crate) fn start<P, D>(
    input: String,
    target: Target,
    cancel: Arc<AtomicBool>,
    progress: P,
    done: D,
) -> Result<(), String>
where
    P: Fn(f64) + Send + 'static,
    D: Fn(Prepared) + Send + 'static,
{
    let work = move || {
        let output = match recoded_path() {
            Ok(output) => output,
            Err(err) => return done(Prepared::Failed(err)),
        };
        let mut last = -1;
        let result = piirit_video::recode(Path::new(&input), &output, &target, |fraction| {
            // Whole percents: there is nothing finer to show.
            #[allow(clippy::cast_possible_truncation)]
            let percent = (fraction * 100.0) as i32;
            if percent != last {
                last = percent;
                progress(fraction);
            }
            !cancel.load(Ordering::Relaxed)
        });
        let output_text = output.to_string_lossy().into_owned();
        match result {
            Ok(Outcome::Done) => done(Prepared::Made(output_text)),
            Ok(Outcome::Cancelled) => {
                discard(&output_text);
                done(Prepared::Cancelled);
            }
            Err(err) => {
                discard(&output_text);
                done(Prepared::Failed(err));
            }
        }
    };
    std::thread::Builder::new()
        .name("piirit-recode".into())
        .spawn(work)
        .map(drop)
        .map_err(|err| format!("could not start recoding: {err}"))
}

#[cfg(test)]
mod tests {
    use super::{discard, is_video, recoded_name};

    #[test]
    fn the_videos_a_phone_records_are_recognised_by_suffix() {
        for path in [
            "/home/user/Videos/clip.mp4",
            "/home/user/Videos/CLIP.MP4",
            "export.m4v",
            "IMG_0001.MOV",
            "old phone.3gp",
            "older phone.3g2",
        ] {
            assert!(is_video(path), "{path}");
        }
        for path in [
            "movie.mkv",
            "movie.webm",
            "movie.avi",
            "photo.jpg",
            "notes",
            "",
        ] {
            assert!(!is_video(path), "{path}");
        }
    }

    #[test]
    fn a_recoded_video_keeps_its_name_with_the_new_suffix() {
        assert_eq!(recoded_name("IMG_0001.MOV"), "IMG_0001.mp4");
        assert_eq!(
            recoded_name("holiday at the lake.mp4"),
            "holiday at the lake.mp4"
        );
        assert_eq!(recoded_name("no suffix"), "no suffix.mp4");
        assert_eq!(recoded_name(""), "video.mp4");
    }

    #[test]
    fn only_a_recoded_video_can_be_discarded() {
        let picked = std::env::temp_dir().join(format!("piirit-picked-{}.mp4", std::process::id()));
        std::fs::write(&picked, b"the reader's own video").expect("write");
        discard(&picked.to_string_lossy());
        assert!(picked.exists(), "a picked file is the reader's, not ours");
        let _ = std::fs::remove_file(&picked);
    }
}
