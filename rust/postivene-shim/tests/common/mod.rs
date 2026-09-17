//! Reading back what the fake core was asked.

// Not every test uses every helper.
#![allow(dead_code)]

use std::cell::RefCell;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Once;

use qmetaobject::{qml_register_enum, QEnum, QString};
use serde_json::Value;

/// `DBus.SystemBus`, which `NetworkWatch.qml` names and a `.qml` stub
/// cannot express: QML forbids capitalised property names, and a
/// registered enum is how the other stub namespaces do it (see the two in
/// `qml_pages.rs`).
#[derive(QEnum)]
#[repr(u8)]
pub enum DBus {
    SessionBus = 0,
    SystemBus = 1,
}

/// Register the stub `Nemo.DBus` enum, once per process. Every test that
/// loads the window needs it: the window holds a `NetworkWatch`, and a
/// component whose `bus:` does not resolve takes the window down with it.
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

/// `Cover.Active` and the rest, which `CoverPage.qml` compares its own
/// `status` against. A registered enum for the same reason as `DBus`
/// above: QML before 5.10 cannot declare one, and the device floor is
/// Qt 5.6. The numbering is Silica's own order; only that both sides of
/// the comparison come from here matters, since on a device both come
/// from Silica.
#[derive(QEnum)]
#[repr(u8)]
pub enum Cover {
    Inactive = 0,
    Activating = 1,
    Active = 2,
    Deactivating = 3,
}

/// Register the stub `Cover` enum, once per process. Needed by anything
/// that loads `qml/cover/CoverPage.qml`.
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

/// A journal path with nothing in it yet. The temp directory is keyed by
/// process id, and a recycled one otherwise leaves the last run's calls in
/// place -- which reads as this run having made them.
pub fn fresh_journal(temp: &Path) -> PathBuf {
    let journal = temp.join("journal.jsonl");
    let _ = std::fs::remove_file(&journal);
    journal
}

/// The stub Silica module tree, for `QmlEngine::add_import_path`.
pub fn stubs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/silica-stubs")
}

/// A `file://` URL for one of the app's shared components.
pub fn component_url(name: &str) -> String {
    format!(
        "file://{}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/components")
            .join(name)
            .display()
    )
}

/// A `file://` URL for one of the app's pages.
pub fn page_url(name: &str) -> String {
    format!(
        "file://{}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/pages")
            .join(name)
            .display()
    )
}

/// A copy of the app's QML tree with Silica's `EnterKey` attached property
/// stripped out of it.
///
/// `EnterKey` is what decides which key the virtual keyboard shows instead
/// of Return, and it has no stub: attached types cannot be written in QML,
/// and `qmetaobject` passes a null attached-properties function for every
/// type it registers. A page that uses one therefore cannot be loaded
/// headlessly at all -- the engine reports "Non-existent attached object"
/// and hands back nothing. Dropping the two lines that mention it leaves
/// the rest of the page exactly as shipped, which is what a test loading
/// this copy is asserting about.
///
/// The copy is keyed by process id and rebuilt each time, so it never
/// serves a stale page from an earlier run.
pub fn qml_tree_without_enter_key() -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    let target = std::env::temp_dir().join(format!("postivene-qml-{}", std::process::id()));
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

/// A `file://` URL for a page inside a copied tree.
pub fn page_url_in(tree: &Path, name: &str) -> String {
    format!("file://{}", tree.join("pages").join(name).display())
}

/// The committed art, which the intro and welcome pages draw from.
pub fn art_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/art")
}

/// Width, height, bit depth, colour type and interlace byte of a committed
/// PNG, read straight out of its IHDR rather than through an image crate.
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

/// A picture that really is one, so an `Image` can reach `Ready`. Any
/// committed PNG would do; this is the one that is certainly there.
pub fn a_real_picture() -> String {
    art_dir()
        .join("faces-portrait.png")
        .canonicalize()
        .expect("the committed art is there")
        .display()
        .to_string()
}

/// Each step and what it recorded. One step per tick: the shim answers
/// asynchronously and `single_shot` only handles whole seconds.
pub type Steps = Rc<RefCell<Vec<(String, String)>>>;

/// Push one step's result onto [`Steps`].
// By value rather than by reference: every call site hands over a `QString`
// the `call!` macro has just built, and asking each of them to borrow it
// buys nothing.
#[allow(clippy::needless_pass_by_value)]
pub fn record(steps: &Steps, label: &str, value: QString) {
    steps
        .borrow_mut()
        .push((label.to_string(), value.to_string()));
}

/// What one labelled step recorded, or a legible stand-in when the step
/// never ran at all.
pub fn value_of<'a>(steps: &'a [(String, String)], label: &str) -> &'a str {
    steps
        .iter()
        .find(|(name, _)| name == label)
        .map_or("<step did not run>", |(_, value)| value.as_str())
}
