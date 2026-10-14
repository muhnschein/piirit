//! Capture (photo, video, voice message): from make until sent.
//!
//! Core copies sent files into blob dir: capture needed only until send lands. Lives in app
//! cache dir (inside sandbox grant), named by kind + time; name is what other end sees.
//! Discard bounded to captures dir: page can name any attachment path, gallery picks stay.
//!
//! Leftovers page never discards (chat closed mid-send, app stopped) swept at first `Captures`
//! of run (chat list's, at app start): files over a day old here and in recoded-video dir
//! (`video.rs`). Nothing of this run's is that old, so no in-flight send loses its file.

use std::path::Path;
use std::sync::Once;
use std::time::{Duration, SystemTime};

use qmetaobject::*;

use crate::qr::cache_file;

/// Captures dir in cache.
const CAPTURES_DIR: &str = "captures";

/// Sweep age: older than any send takes, young enough that leftovers don't sit on phone long.
const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// Hands out capture paths; takes them back.
///
/// ```qml
/// Captures { id: captures }
/// // camera.imageCapture.captureToLocation(captures.new_path("photo", "jpg"))
/// // ... sent: captures.discard(path)
/// ```
#[derive(QObject, Default)]
pub struct Captures {
    base: qt_base_class!(trait QObject),

    /// Fresh path for `kind`, `extension`; dir made if absent.
    /// Empty on failure (`error` says why).
    pub new_path: qt_method!(fn(&mut self, kind: QString, extension: QString) -> QString),
    /// Remove `path` if capture; else leave alone.
    pub discard: qt_method!(fn(&mut self, path: QString)),
    /// Captures dir not made.
    pub error: qt_signal!(message: QString),

    /// Starts sweep on first make.
    _sweep: Sweep,
}

/// Starts sweep at make (QML: `Default`).
struct Sweep;

impl Default for Sweep {
    fn default() -> Self {
        sweep_once();
        Self
    }
}

/// Once per run, own thread: cache shares flash with everything, chat list builds meanwhile.
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

/// Remove files in `dir` older than [`STALE_AFTER`] before `now`.
/// Dirs, links, files after `now` (clock put back) stay.
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
    /// Fresh capture path.
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

/// File directly in `dir`: what `discard` may remove.
fn is_capture(path: &Path, dir: &Path) -> bool {
    path.parent() == Some(dir) && path.file_name().is_some()
}

/// `photo-20260904-151212.jpg`: kind, local time, extension. File's name wherever it goes,
/// so must read well in chat.
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

    /// Leftovers go at a day old, nothing younger: a send of this run's may still read it.
    #[test]
    fn the_sweep_takes_only_files_older_than_a_day() {
        let dir = std::env::temp_dir().join(format!("piirit-sweep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("deeper")).expect("temp dir");
        let left = dir.join("photo-20260904-151212.jpg");
        std::fs::write(&left, b"jpeg").expect("write");
        let inside = dir.join("deeper").join("kept.jpg");
        std::fs::write(&inside, b"jpeg").expect("write");

        let now = std::time::SystemTime::now();
        sweep(&dir, now);
        assert!(left.exists(), "a file written just now was swept");

        let almost = STALE_AFTER
            .checked_sub(std::time::Duration::from_secs(60))
            .expect("a day is longer than a minute");
        sweep(&dir, now + almost);
        assert!(left.exists(), "a file under a day old was swept");

        let later = STALE_AFTER + std::time::Duration::from_secs(60);
        sweep(&dir, now + later);
        assert!(!left.exists(), "a file over a day old was left");
        assert!(inside.exists(), "the sweep went into a directory");

        sweep(&dir.join("missing"), now + later);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
