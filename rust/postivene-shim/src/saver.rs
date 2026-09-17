//! A copy of an attachment where the reader can find it again.
//!
//! Everything a chat receives lives in the core's blob directory, which is
//! the app's own and goes with it, and where the core names a file by its
//! content rather than by what the sender called it. Saving is one copy
//! into a folder the reader looks in -- Pictures, Videos, or the one they
//! chose for files -- under the name the sender gave the file, or that
//! name and a number when it is taken.

use qmetaobject::*;

use crate::chat::local_path;

/// Copies a file out of the core's blob directory.
///
/// ```qml
/// FileSaver { id: saver; onSaved: notice.show(qsTr("Saved")) }
/// MenuItem { onClicked: saver.save_as(page.fileUrl, StandardPaths.pictures, page.fileName) }
/// ```
#[derive(QObject, Default)]
pub struct FileSaver {
    base: qt_base_class!(trait QObject),

    /// Copy the file at `file_url` -- a `file://` URL or a plain path --
    /// into `folder`, which is made if it does not exist, under the name
    /// the file has where it is. Answers on `saved` or `error`.
    pub save: qt_method!(fn(&mut self, file_url: QString, folder: QString)),
    /// [`Self::save`], under `name` rather than the file's own: the name
    /// the sender gave it, which the core does not keep on the file. An
    /// empty name, or one that is only a path, falls back to the file's
    /// own.
    pub save_as: qt_method!(fn(&mut self, file_url: QString, folder: QString, name: QString)),
    /// The copy is at `path`.
    pub saved: qt_signal!(path: QString),
    /// The copy could not be made. The message names what went wrong.
    pub error: qt_signal!(message: QString),
}

impl FileSaver {
    /// Copy the file into the folder under its own name.
    pub fn save(&mut self, file_url: QString, folder: QString) {
        self.save_as(file_url, folder, QString::default());
    }

    /// Copy the file into the folder under `name`; see the declaration.
    pub fn save_as(&mut self, file_url: QString, folder: QString, name: QString) {
        match copy_into(
            &local_path(&file_url.to_string()),
            &folder.to_string(),
            &name.to_string(),
        ) {
            Ok(path) => self.saved(path.into()),
            Err(message) => self.error(message.into()),
        }
    }
}

/// Copy `source` into `folder` under `name` -- or, when that is empty or
/// nothing but separators, under the source's own name -- or under that
/// name and a number when a different file already has it. Answers with
/// the path written.
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

/// The last component of `name`, so that a name is only ever a name: a
/// sender's `../x` or `a/b` lands in the folder asked for and nowhere
/// else. `None` when nothing is left of it, or when it names only the
/// current or parent directory.
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

/// `name`, or `name (2)`, `name (3)`... before the extension: the first
/// that is not already in the folder.
fn free_name(folder: &std::path::Path, name: &str) -> std::path::PathBuf {
    let first = folder.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, extension) = match name.rfind('.') {
        // A leading dot is a hidden file's name, not an extension.
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
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
        let temp = std::env::temp_dir().join(format!("postivene-saver-{}", std::process::id()));
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

        // A URL, as the page holds the file, and a name with no extension.
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

        assert!(copy_into("", &pictures.to_string_lossy(), "").is_err());
        assert!(copy_into(&source.to_string_lossy(), "", "").is_err());
        assert!(copy_into(
            &temp.join("missing.png").to_string_lossy(),
            &pictures.to_string_lossy(),
            ""
        )
        .is_err());
        let _ = std::fs::remove_dir_all(&temp);
    }

    /// The core keeps a received file under a name of its own -- its
    /// hash, these days -- and the name the sender gave it beside the
    /// message. A copy is under the sender's name, and a sender's name
    /// that tries to be a path is only ever its last part.
    #[test]
    fn a_saved_file_takes_the_name_the_sender_gave_it() {
        let temp =
            std::env::temp_dir().join(format!("postivene-saver-named-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).expect("temp dir");
        let blob = temp.join("5d41402abc4b2a76b9719d911017c592.pdf");
        std::fs::write(&blob, b"%PDF").expect("write blob");
        let files = temp.join("Documents").join("Postivene");
        let files_str = files.to_string_lossy().into_owned();

        let named = copy_into(&blob.to_string_lossy(), &files_str, "minutes.pdf").expect("copy");
        assert_eq!(named, files.join("minutes.pdf").to_string_lossy());
        assert_eq!(std::fs::read(&named).expect("read copy"), b"%PDF");

        let again = copy_into(&blob.to_string_lossy(), &files_str, "minutes.pdf").expect("copy");
        assert_eq!(again, files.join("minutes (2).pdf").to_string_lossy());

        // A name that is a path stays in the folder asked for.
        let escaped =
            copy_into(&blob.to_string_lossy(), &files_str, "../../escaped.pdf").expect("copy");
        assert_eq!(escaped, files.join("escaped.pdf").to_string_lossy());

        // No name, or nothing usable in it: the blob's own.
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
