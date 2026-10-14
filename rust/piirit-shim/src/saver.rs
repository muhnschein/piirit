//! Attachment copy for the reader.
//!
//! Blob dir is app's own (goes with it) and core names files by content. Save = one copy to
//! Piirit's folder in Downloads (`qml/components/AttachmentSaver.qml`) under sender's name,
//! numbered if taken. Own thread: big video copy on Qt thread froze app.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use qmetaobject::*;

use crate::chat::local_path;

/// Copy file out of blob dir.
///
/// ```qml
/// FileSaver { id: saver; onSaved: notice.show(qsTr("Saved")) }
/// MenuItem { enabled: !saver.busy
///            onClicked: saver.save_as(page.fileUrl, StandardPaths.download, page.fileName) }
/// ```
#[derive(QObject, Default)]
pub struct FileSaver {
    base: qt_base_class!(trait QObject),

    /// True until queued saves answered.
    pub busy: qt_property!(bool; NOTIFY busy_changed),
    /// Emitted when [`Self::busy`] changes.
    pub busy_changed: qt_signal!(),

    /// Copy `file_url` (`file://` or path) to `folder`, made if missing.
    /// Answers `saved`/`error`, in ask order.
    pub save: qt_method!(fn(&mut self, file_url: QString, folder: QString)),
    /// [`Self::save`] under `name` (sender's). Empty or path-only: file's own.
    pub save_as: qt_method!(fn(&mut self, file_url: QString, folder: QString, name: QString)),
    /// The copy is at `path`.
    pub saved: qt_signal!(path: QString),
    /// Copy failed; message says why.
    pub error: qt_signal!(message: QString),

    /// Queued saves, oldest first.
    waiting: VecDeque<Request>,
}

/// Queued save: [`copy_into`] args.
struct Request {
    source: String,
    folder: String,
    name: String,
}

impl FileSaver {
    /// Copy under file's own name.
    pub fn save(&mut self, file_url: QString, folder: QString) {
        self.save_as(file_url, folder, QString::default());
    }

    /// Copy under `name`.
    pub fn save_as(&mut self, file_url: QString, folder: QString, name: QString) {
        self.waiting.push_back(Request {
            source: local_path(&file_url.to_string()),
            folder: folder.to_string(),
            name: name.to_string(),
        });
        // One copy at a time: answers in ask order.
        if !self.busy {
            self.busy = true;
            self.busy_changed();
            self.next();
        }
    }

    /// Start oldest queued save; none left: not busy.
    fn next(&mut self) {
        while let Some(request) = self.waiting.pop_front() {
            let ptr: QPointer<Self> = QPointer::from(&*self);
            let done = queued_callback(move |result: Result<String, String>| {
                // Page closed mid-copy: copy still made.
                let Some(this) = ptr.as_pinned() else { return };
                match result {
                    Ok(path) => this.borrow().saved(path.into()),
                    Err(message) => this.borrow().error(message.into()),
                }
                this.borrow_mut().next();
            });
            let started = std::thread::Builder::new()
                .name("piirit-save".into())
                .spawn(move || {
                    done(copy_into(&request.source, &request.folder, &request.name));
                });
            match started {
                Ok(_) => return,
                Err(err) => self.error(format!("could not start saving: {err}").into()),
            }
        }
        self.busy = false;
        self.busy_changed();
    }
}

/// Held while picking name and copying: else two savers (two pages) find same name free and
/// overwrite each other.
static COPYING: Mutex<()> = Mutex::new(());

/// Copy `source` to `folder` as `name` (empty or only separators: source's own), numbered if
/// another file has it. Returns path written.
pub(crate) fn copy_into(source: &str, folder: &str, name: &str) -> Result<String, String> {
    if source.is_empty() {
        return Err("nothing to save".to_string());
    }
    if folder.is_empty() {
        return Err("nowhere to save to".to_string());
    }
    let source = std::path::Path::new(source);
    let name = match plain_name(name) {
        Some(name) => name,
        None => source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| format!("{} has no file name", source.display()))?,
    };
    let folder = std::path::Path::new(folder);
    std::fs::create_dir_all(folder)
        .map_err(|err| format!("cannot create {}: {err}", folder.display()))?;
    // Poisoned lock: safe, next save re-picks name.
    let _copying = COPYING.lock().unwrap_or_else(PoisonError::into_inner);
    let target = free_name(folder, &name);
    std::fs::copy(source, &target).map_err(|err| {
        format!(
            "cannot copy {} to {}: {err}",
            source.display(),
            target.display()
        )
    })?;
    Ok(target.to_string_lossy().into_owned())
}

/// Last part of `name`: `../x` stays in folder. `None` if empty or `.`/`..`.
fn plain_name(name: &str) -> Option<String> {
    let last = name
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();
    match last {
        "" | "." | ".." => None,
        last => Some(last.to_string()),
    }
}

/// `name (2)`, `name (3)`... before extension: first free.
fn free_name(folder: &std::path::Path, name: &str) -> std::path::PathBuf {
    let first = folder.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, extension) = match name.rfind('.') {
        // Leading dot: hidden file, not extension.
        // `rfind` index is char boundary.
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    (2..=u32::MAX)
        .map(|number| folder.join(format!("{stem} ({number}){extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::{copy_into, plain_name};

    #[test]
    fn a_saved_file_keeps_its_name_and_a_second_copy_gets_a_number() {
        let temp = std::env::temp_dir().join(format!("piirit-saver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).expect("temp dir");
        let source = temp.join("holiday photo.jpg");
        std::fs::write(&source, b"jpeg").expect("write source");
        let pictures = temp.join("Pictures");

        let first = copy_into(&source.to_string_lossy(), &pictures.to_string_lossy(), "")
            .expect("first copy");
        assert_eq!(first, pictures.join("holiday photo.jpg").to_string_lossy());
        assert_eq!(std::fs::read(&first).expect("read copy"), b"jpeg");

        let second = copy_into(&source.to_string_lossy(), &pictures.to_string_lossy(), "")
            .expect("second copy");
        assert_eq!(
            second,
            pictures.join("holiday photo (2).jpg").to_string_lossy()
        );

        // URL as page holds it; no extension.
        let plain = temp.join("notes");
        std::fs::write(&plain, b"n").expect("write plain");
        let url = format!("file://{}", plain.display());
        let saved = copy_into(
            &crate::chat::local_path(&url),
            &pictures.to_string_lossy(),
            "",
        )
        .expect("copy from a url");
        assert_eq!(saved, pictures.join("notes").to_string_lossy());

        copy_into("", &pictures.to_string_lossy(), "").unwrap_err();
        copy_into(&source.to_string_lossy(), "", "").unwrap_err();
        copy_into(
            &temp.join("missing.png").to_string_lossy(),
            &pictures.to_string_lossy(),
            "",
        )
        .unwrap_err();
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// Sender's name used; path-y name keeps last part.
    #[test]
    fn a_saved_file_takes_the_name_the_sender_gave_it() {
        let temp = std::env::temp_dir().join(format!("piirit-saver-named-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).expect("temp dir");
        let blob = temp.join("5d41402abc4b2a76b9719d911017c592.pdf");
        std::fs::write(&blob, b"%PDF").expect("write blob");
        let files = temp.join("Downloads");
        let files_str = files.to_string_lossy().into_owned();

        let named = copy_into(&blob.to_string_lossy(), &files_str, "minutes.pdf").expect("copy");
        assert_eq!(named, files.join("minutes.pdf").to_string_lossy());
        assert_eq!(std::fs::read(&named).expect("read copy"), b"%PDF");

        let again = copy_into(&blob.to_string_lossy(), &files_str, "minutes.pdf").expect("copy");
        assert_eq!(again, files.join("minutes (2).pdf").to_string_lossy());

        // Path-y name stays in asked folder.
        let escaped =
            copy_into(&blob.to_string_lossy(), &files_str, "../../escaped.pdf").expect("copy");
        assert_eq!(escaped, files.join("escaped.pdf").to_string_lossy());

        // No usable name: blob's own.
        let unnamed = copy_into(&blob.to_string_lossy(), &files_str, "  ").expect("copy");
        assert_eq!(
            unnamed,
            files
                .join("5d41402abc4b2a76b9719d911017c592.pdf")
                .to_string_lossy()
        );

        assert_eq!(plain_name("a/b/c.txt").as_deref(), Some("c.txt"));
        assert_eq!(plain_name("c:\\folder\\d.txt").as_deref(), Some("d.txt"));
        assert_eq!(
            plain_name(" name with spaces.md ").as_deref(),
            Some("name with spaces.md")
        );
        assert_eq!(plain_name("."), None);
        assert_eq!(plain_name(".."), None);
        assert_eq!(plain_name("dir/"), None);
        assert_eq!(plain_name(""), None);
        let _ = std::fs::remove_dir_all(&temp);
    }
}
