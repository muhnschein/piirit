//! Picture or smaller video left in cache: what removes it. Page discards capture when sent,
//! replaced, cancelled. Leaks: chat closed with capture on bar (page clears it); chat closed
//! mid-send (core may still read file: left to sweep). Sweep: captures and recoded dirs older than
//! 1 day, at first `Captures` of run.
//!
//! Page loaded from QML tree copy with `EnterKey` taken out, as in `qml_send_file.rs`.

// Qt harness: see qml_conversation_open.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: 1,
                chatId: 1,
                status: PageStatus.Activating
            })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.status = PageStatus.Active
            return 'ok'
        }
        // The page going, the way a page popped off the stack goes.
        function close() { loader.setSource('', {}); return 'ok' }
        function attach(path) { loader.item.attach(path); return '' + loader.item.attachmentPath }
        function send() { loader.item.sendCurrentText(); return 'ok' }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
    }
";

/// File at `path`, last written `age` ago.
fn a_file(path: &Path, age: Duration) {
    std::fs::write(path, b"made here").expect("write a test file");
    let when = SystemTime::now()
        .checked_sub(age)
        .expect("a time that far back");
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open a test file")
        .set_modified(when)
        .expect("date a test file");
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_closed_chat_leaves_no_capture_behind_and_old_ones_are_swept() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-cleanup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let cache = temp.join("cache");
    let captures = cache.join("piirit/piirit/captures");
    let recoded = cache.join("piirit/piirit/recoded");
    std::fs::create_dir_all(&captures).expect("create the captures directory");
    std::fs::create_dir_all(&recoded).expect("create the recoded directory");

    let day = Duration::from_secs(24 * 60 * 60);
    // Left by earlier run: two days old, swept.
    let old_capture = captures.join("photo-20260101-120000.jpg");
    a_file(&old_capture, day * 2);
    let old_video = recoded.join("video-1-0.mp4");
    a_file(&old_video, day * 2);
    // An hour old: kept.
    let young_capture = captures.join("voice-20260101-120000.mp3");
    a_file(&young_capture, Duration::from_secs(60 * 60));
    // Pages handed these, as if camera just took them.
    let closed = captures.join("photo-closed.jpg");
    a_file(&closed, Duration::ZERO);
    let sending = captures.join("photo-sending.jpg");
    a_file(&sending, Duration::ZERO);

    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("XDG_CACHE_HOME", &cache);
        // Long enough: page gone before core answers.
        std::env::set_var("PIIRIT_FAKE_SEND_DELAY_MS", "3000");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

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

    let page = common::page_url_in(&tree, "ConversationPage.qml");
    let exists = |path: &PathBuf| path.exists().to_string();

    let mut plan = common::Plan::default();

    // Capture on bar, chat closed.
    let (url, path) = (page.clone(), closed.clone());
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("load", call!("load", QString::from(url.clone()))));
        (*steps_ptr).push((
            "attached",
            call!("attach", QString::from(path.to_string_lossy().as_ref())),
        ));
    });

    let (old, video, young) = (old_capture, old_video, young_capture);
    plan.after(1, move || unsafe {
        // Sweep has had a second on its thread.
        (*steps_ptr).push(("old-capture", exists(&old)));
        (*steps_ptr).push(("old-video", exists(&video)));
        (*steps_ptr).push(("young-capture", exists(&young)));
        (*steps_ptr).push(("close", call!("close")));
    });

    // Capture sent, chat closed before core answered.
    let (url, gone, path) = (page, closed.clone(), sending.clone());
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("closed-capture", exists(&gone)));
        (*steps_ptr).push(("load-again", call!("load", QString::from(url.clone()))));
        (*steps_ptr).push((
            "attached-again",
            call!("attach", QString::from(path.to_string_lossy().as_ref())),
        ));
    });

    plan.after(1, move || unsafe {
        (*steps_ptr).push(("send", call!("send")));
        (*steps_ptr).push((
            "sending",
            call!("get", QString::from("messages"), QString::from("sending")),
        ));
        (*steps_ptr).push(("close-again", call!("close")));
    });

    let path = sending;
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("sending-capture", exists(&path)));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("attached"),
        closed.to_string_lossy(),
        "the capture did not go on the bar. {context}"
    );
    assert_eq!(
        value("closed-capture"),
        "false",
        "a capture left on the bar stayed in the cache after its chat was \
         closed, where nothing will ever send or remove it. {context}"
    );
    assert_eq!(
        (value("old-capture"), value("old-video")),
        ("false".to_string(), "false".to_string()),
        "a capture or a recoded video two days old was not swept as the \
         first Captures of the run was made. {context}"
    );
    assert_eq!(
        value("young-capture"),
        "true",
        "the sweep took a capture an hour old. {context}"
    );
    assert_eq!(
        value("load-again"),
        "ok",
        "the page did not load a second time. {context}"
    );
    assert_eq!(
        value("sending"),
        "true",
        "the send was not under way when the page was closed, so this \
         proves nothing about one that is. {context}"
    );
    assert_eq!(
        value("sending-capture"),
        "true",
        "a capture was removed while the core could still be reading it \
         for the send under way. {context}"
    );
    let _ = std::fs::remove_dir_all(&temp);
}
