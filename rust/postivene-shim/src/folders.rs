//! The folders under a folder, for choosing one to save into.
//!
//! Silica has no folder picker a Harbour app may use, and the platform's
//! own directory model would be one more import to ask for. This lists
//! what a page needs and nothing else: the directories directly under a
//! path, by name, hidden ones left out.

use std::cell::RefCell;

use qmetaobject::*;

use crate::models::{FolderItem, FolderListModel};

/// The directories under `path`, as rows.
///
/// ```qml
/// FolderList { id: folders; path: "/home/nemo/Documents" }
/// SilicaListView { model: folders.rows; delegate: Label { text: model.name } }
/// ```
#[derive(QObject, Default)]
pub struct FolderList {
    base: qt_base_class!(trait QObject),

    /// Whose subfolders to list. Setting it lists them.
    pub path: qt_property!(QString; WRITE set_path NOTIFY path_changed),
    /// Emitted when [`FolderList::path`] changes.
    pub path_changed: qt_signal!(),

    /// One row per subfolder, in name order: `name` and `path`.
    pub rows: qt_property!(RefCell<FolderListModel>; CONST),
    /// How many there are.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    /// Emitted once the rows have been listed again.
    pub rows_changed: qt_signal!(),
    /// Whether `path` could be read at all. False for a folder that is
    /// not there or not the app's to look in, which reads as empty.
    pub readable: qt_property!(bool; READ is_readable NOTIFY rows_changed),

    /// List `path` again.
    pub reload: qt_method!(fn(&mut self)),

    listed_ok: bool,
}

impl FolderList {
    /// Point at another folder and list it.
    pub fn set_path(&mut self, path: QString) {
        if self.path == path {
            return;
        }
        self.path = path;
        self.path_changed();
        self.reload();
    }

    /// The row count; see the declaration.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().row_count()).unwrap_or(u32::MAX)
    }

    /// Whether the last listing worked; see the declaration.
    pub fn is_readable(&self) -> bool {
        self.listed_ok
    }

    /// List the folder; see the declaration.
    pub fn reload(&mut self) {
        let (items, ok) = subfolders(&self.path.to_string());
        self.listed_ok = ok;
        self.rows.borrow_mut().reset_data(items);
        self.rows_changed();
    }
}

/// The directories directly under `path`, by name, without hidden ones,
/// and whether the folder could be read.
fn subfolders(path: &str) -> (Vec<FolderItem>, bool) {
    if path.is_empty() {
        return (Vec::new(), false);
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return (Vec::new(), false);
    };
    let mut items: Vec<FolderItem> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            Some(FolderItem {
                path: entry.path().to_string_lossy().into_owned().into(),
                name: name.into(),
            })
        })
        .collect();
    items.sort_by(|a, b| {
        a.name
            .to_string()
            .to_lowercase()
            .cmp(&b.name.to_string().to_lowercase())
    });
    (items, true)
}

#[cfg(test)]
mod tests {
    use super::subfolders;

    #[test]
    fn subfolders_are_the_visible_directories_by_name() {
        let temp = std::env::temp_dir().join(format!("postivene-folders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        for name in ["zeta", "Alpha", "mid", ".hidden"] {
            std::fs::create_dir_all(temp.join(name)).expect("make a folder");
        }
        std::fs::write(temp.join("a file.txt"), b"x").expect("write a file");

        let (items, ok) = subfolders(&temp.to_string_lossy());
        assert!(ok);
        let names: Vec<String> = items.iter().map(|item| item.name.to_string()).collect();
        assert_eq!(names, ["Alpha", "mid", "zeta"]);
        assert_eq!(
            items[0].path.to_string(),
            temp.join("Alpha").to_string_lossy()
        );

        let (none, ok) = subfolders(&temp.join("missing").to_string_lossy());
        assert!(none.is_empty());
        assert!(!ok);
        assert!(!subfolders("").1);
        let _ = std::fs::remove_dir_all(&temp);
    }
}
