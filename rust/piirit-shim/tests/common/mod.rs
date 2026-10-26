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
/// Copy keyed by pid, rebuilt each run: never stale. Only `.qml` files copied; rest linked
/// (`art/`: 11 MB). Removed when dropped, panic too: 30 tests, one copy each, filled `/tmp`.
pub fn qml_tree_without_enter_key() -> TempTree {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    let target = std::env::temp_dir().join(format!("piirit-qml-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&target);
    copy_qml_without_enter_key(&source, &target);
    TempTree(target)
}

/// Test's own empty `piirit-<name>-<pid>` temp dir, removed when dropped (panic too): some tests
/// write 100 MB there.
pub fn temp_tree(name: &str) -> TempTree {
    let path = std::env::temp_dir().join(format!("piirit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create the test's temp dir");
    TempTree(path)
}

/// Temp dir, removed when dropped; derefs to its root. Keep alive while app uses it.
pub struct TempTree(PathBuf);

impl std::ops::Deref for TempTree {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        // Links removed, not followed: what they point at stays.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Recursive half of [`qml_tree_without_enter_key`]. Directory without `.qml` below: one link.
fn copy_qml_without_enter_key(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create the QML copy");
    for entry in std::fs::read_dir(source).expect("read the QML tree") {
        let entry = entry.expect("read a QML tree entry");
        let (from, to) = (entry.path(), target.join(entry.file_name()));
        if entry.file_type().expect("stat a QML tree entry").is_dir() {
            if holds_qml(&from) {
                copy_qml_without_enter_key(&from, &to);
            } else {
                std::os::unix::fs::symlink(&from, &to).expect("link a QML tree directory");
            }
        } else if from.extension().is_some_and(|kind| kind == "qml") {
            let text = std::fs::read_to_string(&from).expect("read a QML file");
            let kept: Vec<&str> = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("EnterKey."))
                .collect();
            std::fs::write(&to, kept.join("\n")).expect("write a QML file");
        } else {
            std::os::unix::fs::symlink(&from, &to).expect("link a QML tree file");
        }
    }
}

/// Any `.qml` file in `dir` or below.
fn holds_qml(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .expect("read the QML tree")
        .map(|entry| entry.expect("read a QML tree entry").path())
        .any(|path| {
            if path.is_dir() {
                holds_qml(&path)
            } else {
                path.extension().is_some_and(|kind| kind == "qml")
            }
        })
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

/// Each step's recorded result. One step per tick: shim answers async.
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
pub struct Plan(Vec<(u64, u32, Check)>);

/// One step's work. Told whether it is last try; says whether done.
type Check = Box<dyn FnMut(bool) -> bool>;

impl Plan {
    /// `step` `wait` s after one before ended.
    pub fn after(&mut self, wait: u64, step: impl FnOnce() + 'static) {
        let mut step = Some(step);
        self.0.push((
            wait,
            1,
            Box::new(move |_| {
                if let Some(step) = step.take() {
                    step();
                }
                true
            }),
        ));
    }

    /// `check` every `every` s, first `every` s after one before ended, till it says done or
    /// `tries` ran; last try told so (record what is there). Waits on condition, not clock: loaded
    /// runner may take seconds.
    pub fn until(&mut self, every: u64, tries: u32, check: impl FnMut(bool) -> bool + 'static) {
        self.0.push((every, tries.max(1), Box::new(check)));
    }

    pub fn run(mut self) {
        if self.0.is_empty() {
            return;
        }
        let (wait, tries, check) = self.0.remove(0);
        self.fire(wait, tries, check);
    }

    /// Plan's one `single_shot`: each schedules next only once it ran.
    #[allow(clippy::disallowed_methods)] // Plan's own timer.
    fn fire(self, wait: u64, tries: u32, check: Check) {
        let mut pending = Some((check, self));
        single_shot(Duration::from_secs(wait), move || {
            if let Some((mut check, rest)) = pending.take() {
                let last = tries <= 1;
                if check(last) || last {
                    rest.run();
                } else {
                    rest.fire(wait, tries - 1, check);
                }
            }
        });
    }
}
