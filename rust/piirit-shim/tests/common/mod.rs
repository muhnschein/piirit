//! Read back what fake core was asked.

// Not all tests use all helpers.
#![allow(dead_code)]

pub mod http;

use std::cell::RefCell;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Once;
use std::time::Duration;

use qmetaobject::{qml_register_enum, single_shot, QEnum, QString};
use serde_json::Value;

/// `DBus.SystemBus`, named by `NetworkWatch.qml`. Stub `.qml` can't say it; registered enum
/// instead (see `qml_pages.rs`).
#[derive(QEnum)]
#[repr(u8)]
pub enum DBus {
    SessionBus = 0,
    SystemBus = 1,
}

/// Register stub `Nemo.DBus` enum once per process. Needed by any test loading window (holds
/// `NetworkWatch`; unresolved `bus:` takes window down).
pub fn register_dbus_enum() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Ok(uri) = CString::new("Nemo.DBus") else {
            return;
        };
        let Ok(name) = CString::new("DBus") else {
            return;
        };
        qml_register_enum::<DBus>(&uri, 2, 0, &name);
    });
}

/// `Cover.Active` etc., compared by `CoverPage.qml`. Registered enum: QML before 5.10 can't
/// declare one; device floor Qt 5.6. Numbers follow Silica; only both sides coming from here
/// matters.
#[derive(QEnum)]
#[repr(u8)]
pub enum Cover {
    Inactive = 0,
    Activating = 1,
    Active = 2,
    Deactivating = 3,
}

/// Register stub `Cover` enum once per process. Needed by anything loading `CoverPage.qml`.
pub fn register_cover_enum() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Ok(uri) = CString::new("Sailfish.Silica") else {
            return;
        };
        let Ok(name) = CString::new("Cover") else {
            return;
        };
        qml_register_enum::<Cover>(&uri, 1, 0, &name);
    });
}

/// Every recorded call, in order. A line that does not parse is a torn
/// write, not noise: fail rather than drop it and assert on a short list.
pub fn records(journal: &Path) -> Vec<Value> {
    std::fs::read_to_string(journal)
        .unwrap_or_default()
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|err| panic!("journal line is not one JSON object ({err}): {line}"))
        })
        .collect()
}

/// Method name and params per call.
pub fn calls(journal: &Path) -> Vec<(String, Value)> {
    records(journal)
        .into_iter()
        .map(|call| {
            (
                call.get("method")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                call.get("params").cloned().unwrap_or(Value::Null),
            )
        })
        .collect()
}

/// Method names only, in order.
pub fn methods(journal: &Path) -> Vec<String> {
    calls(journal).into_iter().map(|(name, _)| name).collect()
}

/// Journal path, empty. Temp dir keyed by pid: recycled one keeps last run's calls.
pub fn fresh_journal(temp: &Path) -> PathBuf {
    let journal = temp.join("journal.jsonl");
    let _ = std::fs::remove_file(&journal);
    journal
}

/// Stub Silica module tree, for `QmlEngine::add_import_path`.
pub fn stubs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/silica-stubs")
}

/// `file://` URL for app shared component.
pub fn component_url(name: &str) -> String {
    format!(
        "file://{}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/components")
            .join(name)
            .display()
    )
}

/// `file://` URL for app page.
pub fn page_url(name: &str) -> String {
    format!(
        "file://{}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/pages")
            .join(name)
            .display()
    )
}

/// Copy of app QML tree with Silica's `EnterKey` attached property stripped.
///
/// `EnterKey` picks keyboard return key. No stub possible (attached types can't be written in QML;
/// `qmetaobject` passes null attached-properties fn). Page using it fails headless: "Non-existent
/// attached object". Dropping its two lines leaves page as shipped.
///
/// Copy keyed by pid, rebuilt each run: never stale.
pub fn qml_tree_without_enter_key() -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    let target = std::env::temp_dir().join(format!("piirit-qml-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&target);
    copy_qml_without_enter_key(&source, &target);
    target
}

/// Recursive half of [`qml_tree_without_enter_key`].
fn copy_qml_without_enter_key(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create the QML copy");
    for entry in std::fs::read_dir(source).expect("read the QML tree") {
        let entry = entry.expect("read a QML tree entry");
        let (from, to) = (entry.path(), target.join(entry.file_name()));
        if entry.file_type().expect("stat a QML tree entry").is_dir() {
            copy_qml_without_enter_key(&from, &to);
        } else if from.extension().is_some_and(|kind| kind == "qml") {
            let text = std::fs::read_to_string(&from).expect("read a QML file");
            let kept: Vec<&str> = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("EnterKey."))
                .collect();
            std::fs::write(&to, kept.join("\n")).expect("write a QML file");
        } else {
            std::fs::copy(&from, &to).expect("copy a QML file");
        }
    }
}

/// `file://` URL for page in copied tree.
pub fn page_url_in(tree: &Path, name: &str) -> String {
    format!("file://{}", tree.join("pages").join(name).display())
}

/// Committed art, used by intro and welcome pages.
pub fn art_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/art")
}

/// Width, height, bit depth, colour type, interlace byte of committed PNG, from IHDR (no image
/// crate).
pub fn png_header(file: &str) -> (u32, u32, u8, u8, u8) {
    let bytes = std::fs::read(art_dir().join(file))
        .unwrap_or_else(|err| panic!("qml/art/{file} is missing ({err}); it is committed art"));
    assert_eq!(
        &bytes[..8],
        b"\x89PNG\r\n\x1a\n",
        "qml/art/{file} is not a PNG"
    );
    assert_eq!(
        &bytes[12..16],
        b"IHDR",
        "qml/art/{file} does not start with IHDR"
    );
    let at = |offset: usize| {
        u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    (at(16), at(20), bytes[24], bytes[25], bytes[28])
}

/// Real picture so `Image` reaches `Ready`.
pub fn a_real_picture() -> String {
    art_dir()
        .join("logo.png")
        .canonicalize()
        .expect("the committed art is there")
        .display()
        .to_string()
}

/// Each step's recorded result. One step per tick: shim answers async; `single_shot` is whole
/// seconds.
pub type Steps = Rc<RefCell<Vec<(String, String)>>>;

/// Push one step's result onto [`Steps`].
// By value: `call!` builds each `QString`; borrowing buys nothing.
#[allow(clippy::needless_pass_by_value)]
pub fn record(steps: &Steps, label: &str, value: QString) {
    steps
        .borrow_mut()
        .push((label.to_string(), value.to_string()));
}

/// What one labelled step recorded, or stand-in if it never ran.
pub fn value_of<'a>(steps: &'a [(String, String)], label: &str) -> &'a str {
    steps
        .iter()
        .find(|(name, _)| name == label)
        .map_or("<step did not run>", |(_, value)| value.as_str())
}

/// Steps run in turn, each some seconds after last one ended, not at fixed time from start:
/// stalled loop on busy machine then can't run two back to back with no events between.
#[derive(Default)]
pub struct Plan(Vec<(u64, Box<dyn FnOnce()>)>);

impl Plan {
    /// `step` `wait` s after one before ended.
    pub fn after(&mut self, wait: u64, step: impl FnOnce() + 'static) {
        self.0.push((wait, Box::new(step)));
    }

    pub fn run(mut self) {
        if self.0.is_empty() {
            return;
        }
        let (wait, step) = self.0.remove(0);
        let mut pending = Some((step, self));
        single_shot(Duration::from_secs(wait), move || {
            if let Some((step, rest)) = pending.take() {
                step();
                rest.run();
            }
        });
    }
}
