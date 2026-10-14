//! Picked video, recoded smaller before send.
//!
//! Core recodes pictures but sends video as is (`media.rs`); gallery video often exceeds
//! recommended size and some relays (issue #111). On pick: `piirit-video` reads it, plans a
//! smaller one at media quality's rates, recodes on own thread while caption is written; sent
//! only on send. Output in cache beside captures, removed once core copies it or reader drops it;
//! one whose chat closed mid-send swept a day later (`capture.rs`). Shown as picked name with new
//! suffix.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use piirit_video::{Outcome, Target};

use crate::qr::cache_file;

/// Cache subdir for recoded videos.
pub(crate) const RECODED_DIR: &str = "recoded";

/// Suffixes FFmpeg reads here: MP4 and QuickTime. Others sent as is.
const VIDEOS: [&str; 5] = ["mp4", "m4v", "mov", "3gp", "3g2"];

/// Whether `path` is a video, by suffix; a picker says no more.
pub(crate) fn is_video(path: &str) -> bool {
    let suffix = Path::new(path)
        .extension()
        .map(|suffix| suffix.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    VIDEOS.contains(&suffix.as_str())
}

/// Smaller encode plan for `path`, or `None` to send as is: not a readable video, already
/// small, or unreadable (left to core, as before).
///
/// Header only, cheap enough to ask on pick.
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

/// Recipient's name: picked name with new suffix.
pub(crate) fn recoded_name(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "video".into());
    format!("{stem}.mp4")
}

/// Numbers recoded files, process-wide: two chats can each make one.
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Fresh cache path for a recoded video.
fn recoded_path() -> Result<PathBuf, String> {
    let dir = cache_file(&format!("{RECODED_DIR}/.keep"))?
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "no cache directory".to_string())?;
    let number = NEXT.fetch_add(1, Ordering::Relaxed);
    Ok(dir.join(format!("video-{}-{number}.mp4", std::process::id())))
}

/// Remove a recoded video. Limited to recoded dir.
pub(crate) fn discard(path: &str) {
    let Ok(dir) = cache_file(&format!("{RECODED_DIR}/.keep")) else {
        return;
    };
    let path = Path::new(path);
    if path.parent() == dir.parent() && path.file_name().is_some() {
        if let Err(err) = std::fs::remove_file(path) {
            eprintln!("piirit: removing recoded video {}: {err}", path.display());
        }
    }
}

/// How a recoding came out.
pub(crate) enum Prepared {
    /// Smaller file.
    Made(String),
    /// Reader stopped it; nothing sent.
    Cancelled,
    /// Could not be made; original sent instead.
    Failed(String),
}

/// Start recoding `input` smaller on own thread.
///
/// `progress` gets whole percents (UI not woken per frame), `done` the outcome; both run on
/// that thread, so pass `queued_callback`s. `cancel` stops it.
///
/// # Errors
///
/// When no thread could start; neither is called.
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
            // Whole percents only.
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
