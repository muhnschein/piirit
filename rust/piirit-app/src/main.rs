//! Locates the server binary, the `qml/` directory and the translations,
//! registers the context properties, and hands off to the QML engine.

// See piirit-shim/src/lib.rs: qmetaobject's macros expand to references
// across that crate, and upstream's own examples use the glob.
#![allow(clippy::wildcard_imports)]

use std::path::PathBuf;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod memory_log;
mod translations;

/// `PIIRIT_QML_DIR`, then the installed path, then the source tree.
fn qml_dir() -> PathBuf {
    if let Ok(env) = std::env::var("PIIRIT_QML_DIR") {
        return PathBuf::from(env);
    }
    let installed = PathBuf::from("/usr/share/harbour-piirit/qml");
    if installed.is_dir() {
        return installed;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml")
}

/// Where the compiled catalogs are: `PIIRIT_TRANSLATIONS_DIR`, then
/// the installed path, then the source tree -- where `make translations`
/// puts them, beside the `.ts` files they are built from.
fn translations_dir() -> PathBuf {
    if let Ok(env) = std::env::var("PIIRIT_TRANSLATIONS_DIR") {
        return PathBuf::from(env);
    }
    let installed = PathBuf::from("/usr/share/harbour-piirit/translations");
    if installed.is_dir() {
        return installed;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../translations")
}

fn main() {
    // `PIIRIT_MEMORY_LOG=<seconds>`: what the app and the core weigh,
    // on stderr, for profiling on a device. See docs/BUILDING.md.
    memory_log::start_from_env();

    // Types QML instantiates itself, notably the per-chat message model.
    piirit_shim::register_qml_types();

    let core = QObjectBox::new(DeltaChatCore::default());

    // A `QQuickView`, not a bare `QmlEngine`: Silica's `ApplicationWindow`
    // must be hosted in a view and shown, as `SailfishApp::createView()`
    // does. An engine alone loads the QML but never creates a window.
    let mut view = QQuickView::new();

    // After the view, which is what makes the application object the
    // translator hangs off; before the QML, which is translated as it is
    // built and not again. A language with no catalog gets the English
    // one, for its plural forms; every other string is English already.
    translations::install(&translations_dir().to_string_lossy(), "");

    // Must exist before the QML is sourced, or bindings see undefined.
    view.engine()
        .set_object_property("core".into(), core.pinned());
    // `--rpc-server <path>`, then `PIIRIT_RPC_SERVER`, then the bundled
    // binary -- and never `PATH`; see `piirit_shim::server_path`.
    // Resolved here rather than set by the desktop entry: Sailfish
    // launches `silica-qt5` apps through the invoker, which executes the
    // binary itself, so an `Exec=env FOO=bar app` wrapper is not reliably
    // honoured.
    let server = piirit_shim::server_path(
        std::env::args().skip(1),
        std::env::var("PIIRIT_RPC_SERVER").ok(),
    );
    view.engine()
        .set_property("rpcServerPath".into(), QString::from(server).into());

    let main_qml = qml_dir().join("piirit.qml");
    view.set_source(QString::from(main_qml.to_string_lossy().into_owned()));
    view.show();
    view.engine().exec();

    // The window is gone; so should the server be, by our hand rather
    // than by its own reaction to a closed pipe.
    piirit_shim::shutdown();
}
