//! Speech model download through Qt's network stack, against a loopback host: starts by itself
//! when switched on, stops keeping its part, resumes with `Range` after re-hashing the part,
//! starts over when the host ignores the range, refuses a damaged file, and switching off
//! deletes the model.

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::cell::RefCell;
use std::rc::Rc;

use qmetaobject::*;

mod common;
#[path = "transcripts_support/model_server.rs"]
mod model_server;

use model_server::{model_bytes, sha256_hex, Answer, ModelServer};

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        Transcriber { id: transcriber; enabled: true }
        function report() {
            return transcriber.model_state + '|' + transcriber.model_permille + '|'
                + transcriber.model_error
        }
        function act(what) {
            if (what === 'cancel') transcriber.cancel_download()
            else if (what === 'download') transcriber.download_model()
            else if (what === 'off') transcriber.enabled = false
            else if (what === 'on') transcriber.enabled = true
        }
    }
";

const SIZE: usize = 200_000;
const HELD: usize = 100_000;
const STALE_PART: usize = 50_000;

type Log = Rc<RefCell<Vec<(&'static str, String)>>>;

fn size_of(path: &std::path::Path) -> String {
    std::fs::metadata(path).map_or_else(|_| "none".to_owned(), |meta| meta.len().to_string())
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_model_downloads_resumes_and_is_checked() {
    let temp = common::temp_tree("transcripts-download");
    let speech = temp.join("speech");
    let model = speech.join("ggml-small-q5_1.bin");
    let part = speech.join("ggml-small-q5_1.bin.part");
    let body = model_bytes(SIZE);
    let server = ModelServer::start(
        body.clone(),
        vec![
            Answer::HoldAfter(HELD),
            Answer::Honour,
            Answer::Corrupt,
            Answer::IgnoreRange,
        ],
    );

    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_SPEECH_MODEL_URL", &server.url);
        std::env::set_var("PIIRIT_SPEECH_MODEL_SHA256", sha256_hex(&body));
        std::env::set_var("PIIRIT_SPEECH_MODEL_BYTES", SIZE.to_string());
    }
    piirit_shim::register_qml_types();
    let mut engine = QmlEngine::new();
    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let log: Log = Rc::default();
    let server = Rc::new(server);

    // SAFETY for every block: run inside `exec()`; `engine` outlives it.
    let report = move || unsafe {
        QString::from_qvariant((*engine_ptr).invoke_method("report".into(), &[]))
            .map(|text| text.to_string())
            .unwrap_or_default()
    };
    let act = move |what: &str| unsafe {
        (*engine_ptr).invoke_method("act".into(), &[QString::from(what).to_qvariant()]);
    };
    let state = move || report().split('|').next().unwrap_or_default().to_owned();

    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    // Switched on, none on phone: fetch starts by itself; host stops halfway.
    {
        let (log, server, part) = (log.clone(), server.clone(), part.clone());
        plan.until(1, 30, move |last| {
            let now = report();
            if !now.starts_with("1|500|") && !last {
                return false;
            }
            log.borrow_mut().push(("held", now));
            act("cancel");
            log.borrow_mut().push(("cancelled", report()));
            log.borrow_mut().push(("part after cancel", size_of(&part)));
            server.release();
            act("download");
            true
        });
    }
    // Rest fetched with Range after part re-hashed.
    {
        let (log, model, part) = (log.clone(), model.clone(), part.clone());
        plan.until(1, 30, move |last| {
            if state() != "2" && !last {
                return false;
            }
            log.borrow_mut().push(("resumed", report()));
            let whole = std::fs::read(&model).unwrap_or_default();
            log.borrow_mut().push(("model hash", sha256_hex(&whole)));
            log.borrow_mut().push(("part after resume", size_of(&part)));
            act("off");
            log.borrow_mut().push(("off", report()));
            log.borrow_mut().push(("model after off", size_of(&model)));
            act("on");
            true
        });
    }
    // Switched on again: fetch starts again; host sends a damaged copy.
    {
        let log = log.clone();
        plan.until(1, 30, move |last| {
            if state() != "3" && !last {
                return false;
            }
            log.borrow_mut().push(("damaged", report()));
            log.borrow_mut()
                .push(("part after damaged", size_of(&part)));
            // Part left by an older run; host will ignore the range.
            std::fs::write(&part, vec![7_u8; STALE_PART]).expect("write stale part");
            act("download");
            true
        });
    }
    {
        let log = log.clone();
        plan.until(1, 30, move |last| {
            if state() != "2" && !last {
                return false;
            }
            log.borrow_mut().push(("restarted", report()));
            let whole = std::fs::read(&model).unwrap_or_default();
            log.borrow_mut()
                .push(("model hash after restart", sha256_hex(&whole)));
            unsafe { (*engine_ptr).quit() };
            true
        });
    }
    plan.run();
    engine.exec();

    let log = log.borrow();
    let get = |label: &str| {
        log.iter()
            .find(|(name, _)| *name == label)
            .map_or("<step did not run>".to_owned(), |(_, value)| value.clone())
    };
    let context = format!("log: {log:?}\nranges: {:?}", server.ranges());
    let expected = sha256_hex(&body);

    assert_eq!(get("held"), "1|500|", "{context}");
    assert_eq!(get("cancelled"), "0|0|", "stop is not a failure. {context}");
    assert_eq!(
        get("part after cancel"),
        HELD.to_string(),
        "stop keeps the part. {context}"
    );
    assert_eq!(get("resumed"), "2|1000|", "{context}");
    assert_eq!(
        get("model hash"),
        expected,
        "resumed file is the model. {context}"
    );
    assert_eq!(get("part after resume"), "none", "{context}");
    assert_eq!(get("off"), "0|0|", "{context}");
    assert_eq!(
        get("model after off"),
        "none",
        "switching off deletes the model. {context}"
    );
    let damaged = get("damaged");
    assert!(
        damaged.starts_with("3|") && damaged.contains("damaged"),
        "a copy with the wrong hash is refused. {context}"
    );
    assert_eq!(
        get("part after damaged"),
        "none",
        "a damaged part is not resumed. {context}"
    );
    assert_eq!(get("restarted"), "2|1000|", "{context}");
    assert_eq!(get("model hash after restart"), expected, "{context}");
    assert_eq!(
        server.ranges(),
        vec![
            None,
            Some(format!("bytes={HELD}-")),
            None,
            Some(format!("bytes={STALE_PART}-")),
        ],
        "first fetch whole, resume from the part's end, fresh start whole. {context}"
    );
}
