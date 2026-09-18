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

use qmetaobject::*;

use crate::qr::cache_file;

/// The directory under the cache the captures live in.
const CAPTURES_DIR: &str = "captures";

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
        if is_capture(std::path::Path::new(&path), &dir) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Whether `path` names a file directly inside `dir`: what `discard` is
/// allowed to remove.
fn is_capture(path: &std::path::Path, dir: &std::path::Path) -> bool {
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
    use super::{capture_name, is_capture, now_stamp};

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
}
