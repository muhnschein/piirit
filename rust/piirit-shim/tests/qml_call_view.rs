//! The call's page waits for the browser engine before anything is asked
//! of it.
//!
//! The engine starts on the event loop after its module is first
//! imported -- on the first call of a run, that is when the call's page
//! comes up -- and until it has, it drops what it is told. A microphone
//! grant sent then is lost, and the call opens on the engine's own
//! prompt. So the view holds both the grant and the page until the engine
//! says it is up, and then grants first.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.WebEngine 1.0
    Item {
        Loader { id: holder; width: 540; height: 960 }
        function load(url) {
            WebEngine.ready = false
            holder.setSource(url, { url: 'http://127.0.0.1:4242/index.html?key=ab#startCall' })
            return holder.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function said() {
            var view = holder.item.children[holder.item.children.length - 1]
            return WebEngine.notified.replace(/\n/g, ';') + '|' + holder.item.target + '|' + view.url
        }
        function start() { WebEngine.start(); return 'ok' }
    }
";

#[test]
fn the_page_waits_for_the_engine_and_is_granted_first() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    single_shot(Duration::from_millis(500), move || unsafe {
        (*steps_ptr).push((
            "load",
            call!("load", QString::from(common::component_url("CallView.qml"))),
        ));
        (*steps_ptr).push(("cold", call!("said")));
        (*steps_ptr).push(("start", call!("start")));
        (*steps_ptr).push(("up", call!("said")));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the call view did not load. {context}");
    assert_eq!(
        value("cold"),
        "||",
        "the view granted, or went to the page, before the engine was up. {context}"
    );
    let up = value("up");
    let url = "http://127.0.0.1:4242/index.html?key=ab#startCall";
    assert!(
        up.starts_with("embedui:perms {\"msg\":\"add\",\"uri\":\"http://127.0.0.1:4242\"")
            && up.ends_with(&format!(";|{url}|{url}")),
        "once the engine was up, the page's origin was not granted the \
         microphone and the view pointed at the page: {up}. {context}"
    );
}
