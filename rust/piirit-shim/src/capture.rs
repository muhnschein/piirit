//! Where something made here -- a photo, a video, a voice message -- lives
//! between being made and being sent.
//!
//! The core copies a sent file into its own blob directory, so what the
//! camera or the recorder writes is only needed until the send lands. It
//! goes in the app's own cache directory, inside the sandbox grant, under
//! a name that says what it is and when it was made; that name is what
//! the other end sees the file called. Discarding is bounded to that one
//! directory, so the page can ask for any attachment path to be
//! discarded and a photo picked from the gallery is left where it was.
//!
//! What the page never gets to discard -- a chat closed while its send
//! was under way, an app that was stopped -- is swept up later: as the
//! first `Captures` of a run is made, which is the chat list's as the
//! app starts, whatever in here and in the recoded videos' directory
//! (`video.rs`) is more than a day old goes. Nothing of this run's can
//! be that old by then, so nothing a send could still be reading goes
//! with it.

use std::path::Path;
use std::sync::Once;
use std::time::{Duration, SystemTime};

use qmetaobject::*;

use crate::qr::cache_file;

/// The directory under the cache the captures live in.
const CAPTURES_DIR: &str = "captures";

/// How old a file has to be for the sweep to take it: older than any
/// send takes, and young enough that what was left behind does not sit
/// on the phone for long.
const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// Hands out paths for a capture and takes them back afterwards.
///
/// ```qml
/// Captures { id: captures }
/// // camera.imageCapture.captureToLocation(captures.new_path("photo", "jpg"))
/// // ... sent: captures.discard(path)
/// ```
#[derive(QObject, Default)]
pub struct Captures {
    base: qt_base_class!(trait QObject),

    /// A fresh path for a `kind` of capture -- "photo", "video", "voice"
    /// -- with `extension`, in the captures directory, which is made if
    /// it does not exist. Empty, with `error` said, when there is nowhere
    /// to put one.
    pub new_path: qt_method!(fn(&mut self, kind: QString, extension: QString) -> QString),
    /// Remove the file at `path` when it is a capture; anything else is
    /// left alone.
    pub discard: qt_method!(fn(&mut self, path: QString)),
    /// The captures directory could not be made.
    pub error: qt_signal!(message: QString),

    /// Starts the sweep as the first of these is made.
    _sweep: Sweep,
}

/// What a [`Captures`] holds to start the sweep as it is made: QML makes
/// one through `Default`, and this is the part of it that runs code.
struct Sweep;

impl Default for Sweep {
    fn default() -> Self {
        sweep_once();
        Self
    }
}

/// Sweep, once a run, on a thread of its own: the cache is on the same
/// flash as everything else, and the chat list is being built meanwhile.
fn sweep_once() {
    static SWEPT: Once = Once::new();
    SWEPT.call_once(|| {
        let started = std::thread::Builder::new()
            .name("piirit-sweep".into())
            .spawn(|| {
                let now = SystemTime::now();
                for dir in [CAPTURES_DIR, crate::video::RECODED_DIR] {
                    if let Ok(dir) = cache_file(dir) {
                        sweep(&dir, now);
                    }
                }
            });
        if let Err(err) = started {
            eprintln!("piirit: not sweeping the cache: {err}");
        }
    });
}

/// Remove the files directly in `dir` last written more than
/// [`STALE_AFTER`] before `now`. A directory, a link, or a file written
/// "after" `now` -- a clock put back -- is left alone; a directory that
/// is not there is nothing to do.
fn sweep(dir: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STALE_AFTER);
        if metadata.is_file() && stale {
            if let Err(err) = std::fs::remove_file(&path) {
                eprintln!("piirit: sweeping {}: {err}", path.display());
            }
        }
    }
}

impl Captures {
    /// A fresh path for a capture.
    pub fn new_path(&mut self, kind: QString, extension: QString) -> QString {
        let name = capture_name(&kind.to_string(), &extension.to_string(), now_stamp());
        match cache_file(&format!("{CAPTURES_DIR}/{name}")) {
            Ok(path) => path.to_string_lossy().into_owned().into(),
            Err(err) => {
                self.error(err.into());
                QString::default()
            }
        }
    }

    /// Remove a capture.
    pub fn discard(&mut self, path: QString) {
        let path = crate::chat::local_path(&path.to_string());
        let Ok(dir) = cache_file(CAPTURES_DIR) else {
            return;
        };
        if is_capture(Path::new(&path), &dir) {
            if let Err(err) = std::fs::remove_file(&path) {
                eprintln!("piirit: removing capture {path}: {err}");
            }
        }
    }
}

/// Whether `path` names a file directly inside `dir`: what `discard` is
/// allowed to remove.
fn is_capture(path: &Path, dir: &Path) -> bool {
    path.parent() == Some(dir) && path.file_name().is_some()
}

/// `photo-20260904-151212.jpg`: the kind, the local time it was made,
/// the extension. What the file is called wherever it goes, so it has
/// to be a name a person can read in a chat.
fn capture_name(kind: &str, extension: &str, stamp: String) -> String {
    let kind = kind
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>();
    let extension = extension
        .trim_start_matches('.')
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>();
    let kind = if kind.is_empty() { "capture" } else { &kind };
    if extension.is_empty() {
        format!("{kind}-{stamp}")
    } else {
        format!("{kind}-{stamp}.{extension}")
    }
}

/// The local time now, as `YYYYMMDD-HHMMSS`.
fn now_stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::{capture_name, is_capture, now_stamp, sweep, STALE_AFTER};

    #[test]
    fn a_capture_is_named_by_what_it_is_and_when() {
        let stamp = || "20260904-151212".to_string();
        assert_eq!(
            capture_name("photo", "jpg", stamp()),
            "photo-20260904-151212.jpg"
        );
        // A dotted or odd extension, and a kind with nothing in it.
        assert_eq!(
            capture_name("voice", ".ogg", stamp()),
            "voice-20260904-151212.ogg"
        );
        assert_eq!(capture_name("", "", stamp()), "capture-20260904-151212");
        assert_eq!(
            capture_name("../etc", "p/asswd", stamp()),
            "etc-20260904-151212.passwd"
        );
        let stamp = now_stamp();
        assert_eq!(stamp.len(), 15, "{stamp}");
        assert_eq!(stamp.as_bytes()[8], b'-');
    }

    #[test]
    fn only_a_file_inside_the_captures_directory_can_be_discarded() {
        let dir = std::path::Path::new("/tmp/cache/captures");
        assert!(is_capture(
            std::path::Path::new("/tmp/cache/captures/photo.jpg"),
            dir
        ));
        assert!(!is_capture(
            std::path::Path::new("/home/user/Pictures/holiday.jpg"),
            dir
        ));
        assert!(!is_capture(
            std::path::Path::new("/tmp/cache/captures/deeper/photo.jpg"),
            dir
        ));
        assert!(!is_capture(
            std::path::Path::new("/tmp/cache/captures"),
            dir
        ));
    }

    /// What a closed chat left behind goes once it is a day old, and
    /// nothing younger does: a send of this run's could still be reading
    /// it.
    #[test]
    fn the_sweep_takes_only_files_older_than_a_day() {
        let dir = std::env::temp_dir().join(format!("piirit-sweep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("deeper")).expect("temp dir");
        let left = dir.join("photo-20260904-151212.jpg");
        std::fs::write(&left, b"jpeg").expect("write");
        let inside = dir.join("deeper").join("kept.jpg");
        std::fs::write(&inside, b"jpeg").expect("write");

        // Written just now: young, and kept.
        let now = std::time::SystemTime::now();
        sweep(&dir, now);
        assert!(left.exists(), "a file written just now was swept");

        // Just under a day on: still kept.
        let almost = STALE_AFTER
            .checked_sub(std::time::Duration::from_secs(60))
            .expect("a day is longer than a minute");
        sweep(&dir, now + almost);
        assert!(left.exists(), "a file under a day old was swept");

        // A day and more on: gone, and only it -- not the directory
        // beside it, nor what is in that.
        let later = STALE_AFTER + std::time::Duration::from_secs(60);
        sweep(&dir, now + later);
        assert!(!left.exists(), "a file over a day old was left");
        assert!(inside.exists(), "the sweep went into a directory");

        // Nothing there at all is nothing to do.
        sweep(&dir.join("missing"), now + later);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
