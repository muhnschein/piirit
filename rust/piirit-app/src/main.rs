//! Find server, `qml/`, translations; run QML.

// Stricter lints for shipped code. See piirit-shim/src/lib.rs.
#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects,
        clippy::let_underscore_must_use,
        clippy::panic,
        clippy::panic_in_result_fn,
        clippy::unwrap_in_result,
        clippy::get_unwrap
    )
)]
// qmetaobject macros: glob imports. See piirit-shim/src/lib.rs.
#![allow(clippy::wildcard_imports)]

use std::path::PathBuf;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod memory_log;
mod translations;

/// `PIIRIT_QML_DIR`, installed path, source tree.
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

/// Compiled catalogs: `PIIRIT_TRANSLATIONS_DIR`, installed, source tree.
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
    // First, while single-threaded: sets env the browser engine reads at call start and passes
    // to the H.264 encoder (calls to iPhones).
    piirit_shim::prepare_media_plugins();

    // `PIIRIT_MEMORY_LOG=<seconds>`: RSS to stderr. See docs/BUILDING.md.
    memory_log::start_from_env();

    // Types QML instantiates itself.
    piirit_shim::register_qml_types();

    let core = QObjectBox::new(DeltaChatCore::default());

    // QQuickView, not bare engine: Silica `ApplicationWindow` must be hosted and shown, as
    // `SailfishApp::createView()` does; engine alone never makes a window.
    let mut view = QQuickView::new();

    // After view (makes app object translator hangs off), before QML (translated once, at
    // build). Language without catalog gets English one, for plural forms.
    translations::install(&translations_dir().to_string_lossy(), "");

    // Before QML sourced, or bindings see undefined.
    view.engine()
        .set_object_property("core".into(), core.pinned());
    // `--rpc-server <path>`, then `PIIRIT_RPC_SERVER`, then bundled binary; never PATH.
    // Resolved here, not desktop entry: Sailfish invoker execs binary itself, so `Exec=env`
    // wrappers aren't reliably honoured. See `piirit_shim::server_path`.
    let server = piirit_shim::server_path(
        std::env::args().skip(1),
        std::env::var("PIIRIT_RPC_SERVER").ok(),
    );
    view.engine()
        .set_property("rpcServerPath".into(), QString::from(server).into());

    // Avatars, intro drawings, call backdrop: plain images from shim.
    piirit_shim::install_pictures(view.engine());

    let main_qml = qml_dir().join("piirit.qml");
    view.set_source(QString::from(main_qml.to_string_lossy().into_owned()));
    view.show();
    view.engine().exec();

    // Stop server by our hand, not on closed pipe.
    piirit_shim::shutdown();
}
