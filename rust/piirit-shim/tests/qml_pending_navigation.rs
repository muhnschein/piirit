//! Core-asked move waits for its page on screen.
//!
//! Moves act on stack top. Call page pushed over screen (`CallCenter.qml`): a relay add or
//! chat made mid-call took the call page. Move waits in `PendingNavigation`; `patience`
//! bounds only the busy-stack wait, not call length.

// Qt harness: see qml_share.rs.
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
    import Sailfish.Silica 1.0
    Item {
        // A stack that records what it was asked, and can be busy.
        property QtObject stack: QtObject {
            property bool busy: false
            property string log: ''
            function pop(target, operation) {
                if (target === undefined) {
                    log += 'pop;'
                    return
                }
                log += 'pop:' + target + ':'
                    + (operation === PageStackAction.Immediate ? 'immediate' : 'animated') + ';'
            }
            function replace(page, properties) { log += 'replace:' + page + ';' }
            function replaceAbove(target, page, properties) {
                log += 'replaceAbove:' + target + ':' + page + ';'
            }
        }
        Loader { id: loader }

        function load(url) {
            // Not on screen: a call's page is over the owner. The waits
            // are short, so a test can outlast them.
            loader.setSource(url, {
                stack: stack, ready: false, patience: 200, retryInterval: 50
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function pop() { loader.item.pop(); return stack.log }
        function replace() { loader.item.replace('Next.qml', {}); return stack.log }
        function popTo() { loader.item.popTo('Chats.qml'); return stack.log }
        function replaceAbove() {
            loader.item.replaceAbove(null, 'Chats.qml', {})
            return stack.log
        }
        function ready(on) { loader.item.ready = on; return stack.log }
        function busy(on) { stack.busy = on; return stack.log }
        function log() { return stack.log }
        function pending() { return '' + loader.item.pending }
    }
";

#[test]
fn a_move_waits_for_its_page_to_be_on_screen() {
    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    // `PageStackAction` for pop down.
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let navigation = common::component_url("PendingNavigation.qml");
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    // SAFETY: callbacks run only inside `exec()` on this thread; all they point at outlives it.
    single_shot(Duration::from_millis(0), move || unsafe {
        record!("load", call!("load", QString::from(navigation.clone())));
        // Relay added while call page over owner.
        record!("asked", call!("pop"));
        record!("pending", call!("pending"));
    });

    // Past `patience`: call still up, nothing moved.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!("waited", call!("log"));
        // Call over, page gone: owner on screen.
        record!("on-screen", call!("ready", true));
        record!("pending-after", call!("pending"));

        // Same for replace of owner.
        call!("ready", false);
        record!("replace-asked", call!("replace"));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("replace-on-screen", call!("ready", true));

        // Stack still animating: the wait `patience` bounds.
        call!("busy", true);
        record!("busy-asked", call!("replaceAbove"));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("busy-done", call!("log"));

        // Pop down to page under: at once, no slide back over same screen.
        call!("busy", false);
        call!("ready", false);
        record!("pop-to-asked", call!("popTo"));
        record!("pop-to-on-screen", call!("ready", true));
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

    assert_eq!(value("load"), "ok", "{context}");
    assert_eq!(
        value("asked"),
        "",
        "a move was made over a call's page. {context}"
    );
    assert_eq!(value("pending"), "true", "{context}");
    assert_eq!(
        value("waited"),
        "",
        "a move ran out of patience and was made over a call's page, \
         taking the call with it. {context}"
    );
    assert_eq!(
        value("on-screen"),
        "pop;",
        "a move was not made once its page was on screen again. {context}"
    );
    assert_eq!(value("pending-after"), "false", "{context}");
    assert_eq!(value("replace-asked"), "pop;", "{context}");
    assert_eq!(
        value("replace-on-screen"),
        "pop;replace:Next.qml;",
        "{context}"
    );
    assert_eq!(value("busy-asked"), "pop;replace:Next.qml;", "{context}");
    assert_eq!(
        value("busy-done"),
        "pop;replace:Next.qml;replaceAbove:null:Chats.qml;",
        "a stack that never went quiet held a move for ever. {context}"
    );
    let before_pop_to = value("busy-done");
    assert_eq!(value("pop-to-asked"), before_pop_to, "{context}");
    assert_eq!(
        value("pop-to-on-screen"),
        format!("{before_pop_to}pop:Chats.qml:immediate;"),
        "a pop down was not made at once to the page asked for. {context}"
    );
}
