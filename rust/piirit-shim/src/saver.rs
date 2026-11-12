//! Attachment copy for the reader.
//!
//! Blob dir goes with app; core names by content. Save = copy into Piirit's Downloads folder
//! (`qml/components/AttachmentSaver.qml`) under sender's name, numbered if taken. Own thread:
//! big video on Qt thread froze app.
//!
//! Open in another app = same copy, handed on: other apps' sandboxes can't read blob dir, and
//! blob's name is a hash. Same file already there: that one, no second copy.

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

    /// Copy `file_url` (`file://` or path) to `folder` (made if missing); `saved`/`error` in
    /// order.
    pub save: qt_method!(fn(&mut self, file_url: QString, folder: QString)),
    /// [`Self::save`] under `name` (sender's). Empty or path-only: file's own.
    pub save_as: qt_method!(fn(&mut self, file_url: QString, folder: QString, name: QString)),
    /// [`Self::save_as`] for handing on: copy of same content already there is reused; answers
    /// `opened`.
    pub open_as: qt_method!(fn(&mut self, file_url: QString, folder: QString, name: QString)),
    /// The copy is at `path`.
    pub saved: qt_signal!(path: QString),
    /// The copy to hand on is at `path`.
    pub opened: qt_signal!(path: QString),
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
    /// For handing on: reuse same content, answer `opened`.
    open: bool,
}

impl FileSaver {
    /// Copy under file's own name.
    pub fn save(&mut self, file_url: QString, folder: QString) {
        self.save_as(file_url, folder, QString::default());
    }

    /// Copy under `name`.
    pub fn save_as(&mut self, file_url: QString, folder: QString, name: QString) {
        self.queue(&file_url, &folder, &name, false);
    }

    /// Copy under `name` unless already there, to hand on.
    pub fn open_as(&mut self, file_url: QString, folder: QString, name: QString) {
        self.queue(&file_url, &folder, &name, true);
    }

    fn queue(&mut self, file_url: &QString, folder: &QString, name: &QString, open: bool) {
        self.waiting.push_back(Request {
            source: local_path(&file_url.to_string()),
            folder: folder.to_string(),
            name: name.to_string(),
            open,
        });
        // One at a time: answers in order.
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
            let open = request.open;
            let done = queued_callback(move |result: Result<String, String>| {
                // Page closed mid-copy: copy still made.
                let Some(this) = ptr.as_pinned() else { return };
                match result {
                    Ok(path) if open => this.borrow().opened(path.into()),
                    Ok(path) => this.borrow().saved(path.into()),
                    Err(message) => this.borrow().error(message.into()),
                }
                this.borrow_mut().next();
            });
            let started = std::thread::Builder::new()
                .name("piirit-save".into())
                .spawn(move || {
                    done(copy_into(
                        &request.source,
                        &request.folder,
                        &request.name,
                        request.open,
                    ));
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

/// Held over name pick and copy: else two savers take same free name.
static COPYING: Mutex<()> = Mutex::new(());

/// Copy `source` to `folder` as `name` (empty/separators: source's), numbered if taken. Returns
/// path. `reuse`: file of that name (or numbered) with same content is returned, nothing copied.
pub(crate) fn copy_into(
    source: &str,
    folder: &str,
    name: &str,
    reuse: bool,
) -> Result<String, String> {
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
    // Poisoned: safe, name re-picked.
    let _copying = COPYING.lock().unwrap_or_else(PoisonError::into_inner);
    let (target, there) = free_name(folder, &name, reuse.then_some(source));
    if there {
        return Ok(target.to_string_lossy().into_owned());
    }
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

/// `name (2)`, `name (3)`... before extension: first free (`false`), or first holding what
/// `same_as` holds (`true`).
fn free_name(
    folder: &std::path::Path,
    name: &str,
    same_as: Option<&std::path::Path>,
) -> (std::path::PathBuf, bool) {
    let first = folder.join(name);
    let (stem, extension) = match name.rfind('.') {
        // Leading dot: hidden file, not extension.
        // `rfind` index is char boundary.
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    let numbered =
        (2..=u32::MAX).map(|number| folder.join(format!("{stem} ({number}){extension}")));
    for candidate in std::iter::once(first.clone()).chain(numbered) {
        if !candidate.exists() {
            return (candidate, false);
        }
        if same_as.is_some_and(|source| same_content(source, &candidate)) {
            return (candidate, true);
        }
    }
    (first, false)
}

/// Both readable, byte for byte equal.
fn same_content(one: &std::path::Path, other: &std::path::Path) -> bool {
    use std::io::Read;
    let (Ok(one_size), Ok(other_size)) = (std::fs::metadata(one), std::fs::metadata(other)) else {
        return false;
    };
    if one_size.len() != other_size.len() {
        return false;
    }
    let (Ok(one), Ok(other)) = (std::fs::File::open(one), std::fs::File::open(other)) else {
        return false;
    };
    let mut one = std::io::BufReader::new(one).bytes();
    let mut other = std::io::BufReader::new(other).bytes();
    loop {
        match (one.next(), other.next()) {
            (None, None) => return true,
            (Some(Ok(left)), Some(Ok(right))) if left == right => {}
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_into, plain_name, same_content};

    #[test]
    fn a_saved_file_keeps_its_name_and_a_second_copy_gets_a_number() {
        let temp = std::env::temp_dir().join(format!("piirit-saver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).expect("temp dir");
        let source = temp.join("holiday photo.jpg");
        std::fs::write(&source, b"jpeg").expect("write source");
        let pictures = temp.join("Pictures");

        let first = copy_into(
            &source.to_string_lossy(),
            &pictures.to_string_lossy(),
            "",
            false,
        )
        .expect("first copy");
        assert_eq!(first, pictures.join("holiday photo.jpg").to_string_lossy());
        assert_eq!(std::fs::read(&first).expect("read copy"), b"jpeg");

        let second = copy_into(
            &source.to_string_lossy(),
            &pictures.to_string_lossy(),
            "",
            false,
        )
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
            false,
        )
        .expect("copy from a url");
        assert_eq!(saved, pictures.join("notes").to_string_lossy());

        copy_into("", &pictures.to_string_lossy(), "", false).unwrap_err();
        copy_into(&source.to_string_lossy(), "", "", false).unwrap_err();
        copy_into(
            &temp.join("missing.png").to_string_lossy(),
            &pictures.to_string_lossy(),
            "",
            false,
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

        let named =
            copy_into(&blob.to_string_lossy(), &files_str, "minutes.pdf", false).expect("copy");
        assert_eq!(named, files.join("minutes.pdf").to_string_lossy());
        assert_eq!(std::fs::read(&named).expect("read copy"), b"%PDF");

        let again =
            copy_into(&blob.to_string_lossy(), &files_str, "minutes.pdf", false).expect("copy");
        assert_eq!(again, files.join("minutes (2).pdf").to_string_lossy());

        // Path-y name stays in asked folder.
        let escaped = copy_into(
            &blob.to_string_lossy(),
            &files_str,
            "../../escaped.pdf",
            false,
        )
        .expect("copy");
        assert_eq!(escaped, files.join("escaped.pdf").to_string_lossy());

        // No usable name: blob's own.
        let unnamed = copy_into(&blob.to_string_lossy(), &files_str, "  ", false).expect("copy");
        assert_eq!(
            unnamed,
            files
                .join("5d41402abc4b2a76b9719d911017c592.pdf")
                .to_string_lossy()
        );

        // Handing on: same content there is reused, numbered copy included; other content
        // under same name is left alone.
        let reused =
            copy_into(&blob.to_string_lossy(), &files_str, "minutes.pdf", true).expect("reuse");
        assert_eq!(reused, files.join("minutes.pdf").to_string_lossy());
        let other = temp.join("0cc175b9c0f1b6a831c399e269772661.pdf");
        std::fs::write(&other, b"%PDX").expect("write other blob");
        let fresh =
            copy_into(&other.to_string_lossy(), &files_str, "minutes.pdf", true).expect("copy");
        assert_eq!(fresh, files.join("minutes (3).pdf").to_string_lossy());
        assert_eq!(std::fs::read(&fresh).expect("read copy"), b"%PDX");
        let second =
            copy_into(&other.to_string_lossy(), &files_str, "minutes.pdf", true).expect("reuse");
        assert_eq!(
            second, fresh,
            "same content under a numbered name is not reused"
        );
        // Same length, last byte differs; and a shorter one.
        assert!(!same_content(&blob, &other));
        std::fs::write(&other, b"%PD").expect("shorten");
        assert!(!same_content(&blob, &other));
        assert!(!same_content(&blob, &temp.join("missing.pdf")));
        assert!(!same_content(&temp.join("missing.pdf"), &blob));
        assert!(same_content(&blob, &files.join("minutes (2).pdf")));
        assert_eq!(
            std::fs::read_dir(&files).expect("list").count(),
            5,
            "handing on made copies of what was already there"
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
