//! Webxdc app served over loopback host: what it gets, and to whom.
//!
//! App files must reach core and come back with right bytes and headers; every
//! non-app request refused. Driven over a real socket against fake core, with the
//! same `WebxdcApp` as page.
//!
//! Round trip: page's update reaches core, core emits event, app's held update
//! request wakes with it. Only that request wakes; app info not re-asked. JS
//! can't check this direction.
//!
//! `sendToChat` skips core: host writes file and raises signal for page. Pinned:
//! signal carries file on disk; app can't write outside cache.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string log: ''
        WebxdcApp {
            id: app
            account_id: 1
            message_id: 5
            onError: log += 'error:' + message + ';'
            onGone: log += 'gone;'
            onHanded_over: log += 'to-chat:' + file_path
                                             + '|' + text + ';'
        }
        // Nothing asked of core until it is up; it may be up before this page
        // exists, so same step runs on the signal and on the way in.
        function open() {
            if (core.status === 'ready') {
                app.reload()
                app.start()
            }
        }
        Component.onCompleted: open()
        Connections {
            target: core
            onStatus_changed: open()
            onCore_event: app.handle_event(context_id, kind, payload_json)
        }
        function url() { return app.url }
        function info() {
            return app.name + '|' + app.summary + '|'
                   + (app.icon_path.length > 0 ? 'icon' : 'no-icon')
        }
        function stop() { app.stop(); return app.url }
        function said() { return log }
    }
";

/// One request, one answer, one closed connection. `Err`: nothing listening, as for a
/// stopped app.
fn ask(authority: &str, request: &str) -> Result<String, String> {
    ask_within(authority, request, Duration::from_secs(5))
}

/// Like `ask`, waiting up to `patience`; update requests held until one exists.
fn ask_within(authority: &str, request: &str, patience: Duration) -> Result<String, String> {
    let mut stream =
        std::net::TcpStream::connect(authority).map_err(|err| format!("connect: {err}"))?;
    stream
        .set_read_timeout(Some(patience))
        .map_err(|err| err.to_string())?;
    stream
        .write_all(request.as_bytes())
        .map_err(|err| format!("write: {err}"))?;
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .map_err(|err| format!("read: {err}"))?;
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

/// POST with head and body as separate writes, as real clients send (`ask`
/// sends both at once, hiding a host that answers before reading body).
///
/// Failed write = app sees unreachable host.
fn post_apart(authority: &str, target: &str, body: &str) -> Result<String, String> {
    let mut stream =
        std::net::TcpStream::connect(authority).map_err(|err| format!("connect: {err}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|err| err.to_string())?;
    let head = format!(
        "POST {target} HTTP/1.1\r\nHost: {authority}\r\n\
         Content-Type: application/octet-stream\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|err| format!("write-head: {err}"))?;
    stream.flush().map_err(|err| format!("flush: {err}"))?;
    for part in body.as_bytes().chunks(64 * 1024) {
        stream
            .write_all(part)
            .map_err(|err| format!("write-body: {err}"))?;
    }
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .map_err(|err| format!("read: {err}"))?;
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

/// GET `path` on host, as a phone browser sends it.
fn get(authority: &str, path: &str) -> Result<String, String> {
    ask(
        authority,
        &format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"),
    )
}

/// App's address. Files at its root.
fn authority_of(url: &str) -> String {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    rest.split('/').next().unwrap_or_default().to_string()
}

/// Chat's address, from served bridge. Token appears nowhere else (not app address, not
/// any page): app is told; whatever merely found the port is not.
fn api_of(bridge: &str) -> String {
    bridge
        .split("var API = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default()
        .to_string()
}

#[test]
#[allow(clippy::too_many_lines)]
fn an_app_is_served_to_itself_alone_and_its_updates_reach_the_chat() {
    // Removed at end: test posts 100 MB video into it.
    let temp = common::temp_tree("webxdc");
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let cache = temp.join("cache");
    std::fs::create_dir_all(&cache).expect("create cache dir");

    // Handover from a run that ended before page could save it. Nothing asks for it again,
    // so starting app clears it; else every export would stay on phone.
    let outbox = cache.join("piirit/piirit/webxdc/outbox/5");
    std::fs::create_dir_all(&outbox).expect("create outbox");
    let stale = outbox.join("stale.bin");
    std::fs::write(&stale, "left over").expect("write a stale handover");

    // SAFETY: single-threaded; set before Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Icon written here, not runner's cache; fresh dir proves it.
        std::env::set_var("XDG_CACHE_HOME", &cache);
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    // App's update request, on its own thread: records answer, and whether it
    // came before move.
    let (waited_tx, waited_rx) = mpsc::channel::<String>();
    let mut waited_tx = Some(waited_tx);
    let answered = Arc::new(AtomicBool::new(false));
    let answered_early = Arc::clone(&answered);
    // App's info asks, before move and after.
    let info_calls = move || {
        common::methods(&journal)
            .iter()
            .filter(|name| *name == "get_webxdc_info")
            .count()
            .to_string()
    };
    let info_calls_after = info_calls.clone();

    macro_rules! call {
        ($name:expr) => {{
            let result = (*engine_ptr).invoke_method($name.into(), &[]);
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

    let mut plan = common::Plan::default();

    // SAFETY: fire only while `exec()` runs; `engine` outlives it.
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    plan.after(2, move || unsafe {
        let url = call!("url");
        record!("url", url.clone());
        record!("info", call!("info"));
        let authority = authority_of(&url);
        if authority.is_empty() {
            return;
        }

        record!(
            "index",
            get(&authority, "/index.html").unwrap_or_else(|err| err)
        );
        // Found on phone: bundler writes absolute path into a dir; host keeping app under a
        // prefix answers nothing to it.
        record!(
            "asset",
            get(&authority, "/assets/app.js").unwrap_or_else(|err| err)
        );
        // Address itself, nothing after.
        record!("root", get(&authority, "/").unwrap_or_else(|err| err));
        let bridge = get(&authority, "/webxdc.js").unwrap_or_else(|err| err);
        record!("bridge", bridge.clone());
        let api = api_of(&bridge);
        record!("api", api.clone());
        if api.is_empty() {
            return;
        }

        // App's update request, as bridge makes it once caught up. Held until
        // move; made early so it waits.
        if let Some(waited) = waited_tx.take() {
            let authority = authority.clone();
            let answered = Arc::clone(&answered);
            let request = format!(
                "GET {api}/updates?serial=0&wait=1 HTTP/1.1\r\nHost: {authority}\r\n\
                 Connection: close\r\n\r\n"
            );
            std::thread::spawn(move || {
                let answer = ask_within(&authority, &request, Duration::from_secs(30))
                    .unwrap_or_else(|err| err);
                answered.store(true, Ordering::SeqCst);
                waited.send(answer).ok();
            });
        }

        // Other app guessing port: may get the files its reader was sent anyway, nothing of
        // the chat.
        record!(
            "no-token",
            get(&authority, "/webxdc-api/updates?serial=0").unwrap_or_else(|err| err)
        );
        record!(
            "wrong-token",
            get(
                &authority,
                "/webxdc-api/0123456789abcdef0123456789abcdef/updates?serial=0"
            )
            .unwrap_or_else(|err| err)
        );
        // Page in browser, under its own name.
        record!(
            "wrong-host",
            ask(
                &authority,
                "GET /index.html HTTP/1.1\r\nHost: webxdc.example\r\n\
                 Connection: close\r\n\r\n",
            )
            .unwrap_or_else(|err| err)
        );
        record!(
            "escape",
            get(&authority, "/../../etc/passwd").unwrap_or_else(|err| err)
        );
        record!(
            "missing",
            get(&authority, "/nothing.js").unwrap_or_else(|err| err)
        );

        // App hands file over as `sendToChat` does: body is file, name in query.
        record!(
            "to-chat",
            post_apart(
                &authority,
                &format!("{api}/to-chat?name=notes.txt&text=look"),
                "hi\n"
            )
            .unwrap_or_else(|err| err)
        );

        // Name leaving cache keeps last part only.
        record!(
            "to-chat-escape",
            post_apart(
                &authority,
                &format!("{api}/to-chat?name=..%2F..%2F..%2Fetc%2Fpasswd"),
                "hi\n"
            )
            .unwrap_or_else(|err| err)
        );

        // No file, no words: app's mistake, refused.
        record!(
            "to-chat-empty",
            post_apart(&authority, &format!("{api}/to-chat"), "").unwrap_or_else(|err| err)
        );

        // Shared-size file, separate writes as browser sends. Held nowhere: socket
        // to cache, a chunk at a time.
        let big = "A".repeat(4 * 1024 * 1024);
        record!(
            "to-chat-big",
            post_apart(&authority, &format!("{api}/to-chat?name=photo.png"), &big)
                .unwrap_or_else(|err| err)
        );

        // Past 100 MB. Only disk stops it; a cap would block an app's own video.
        let huge = "A".repeat(101 * 1024 * 1024);
        record!(
            "to-chat-huge",
            post_apart(&authority, &format!("{api}/to-chat?name=video.mp4"), &huge)
                .unwrap_or_else(|err| err)
        );

        record!("info-calls-before", info_calls());
        record!(
            "held-before-send",
            (!answered_early.load(Ordering::SeqCst)).to_string()
        );
        // App sends a move, as `webxdc.js` does.
        let update = "{\"payload\":{\"move\":\"e4\"}}";
        record!(
            "send",
            ask(
                &authority,
                &format!(
                    "POST {api}/send HTTP/1.1\r\nHost: {authority}\r\n\
                     Content-Type: application/json\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n{update}",
                    update.len()
                ),
            )
            .unwrap_or_else(|err| err)
        );
        record!(
            "updates",
            get(&authority, &format!("{api}/updates?serial=0")).unwrap_or_else(|err| err)
        );
    });

    // Send's event had a loop turn to arrive; woken request answered by then.
    plan.after(2, move || unsafe {
        record!(
            "woken",
            waited_rx
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|err| format!("not answered: {err}"))
        );
        record!("info-calls-after", info_calls_after());
        let url = call!("url");
        record!("stopped-url", call!("stop"));
        let authority = authority_of(&url);
        record!(
            "after-stop",
            get(&authority, "/index.html").unwrap_or_else(|err| err)
        );
        record!("log", call!("said"));
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

    assert!(
        value("url").starts_with("http://127.0.0.1:"),
        "the app was not served on the loopback interface. {context}"
    );
    // Nothing played yet: app says nothing about itself.
    assert_eq!(
        value("info"),
        "Checkers||icon",
        "the row's name, summary and icon did not come from the core. {context}"
    );

    let index = value("index");
    assert!(
        index.starts_with("HTTP/1.1 200 OK"),
        "the app's own page was not served. {context}"
    );
    assert!(
        value("root").starts_with("HTTP/1.1 200 OK"),
        "the address itself did not answer with the app's page: {}. \
         {context}",
        value("root")
    );
    // Bundler's absolute path: app asked for it, got nothing; hence root serving.
    let asset = value("asset");
    assert!(
        asset.starts_with("HTTP/1.1 200 OK") && asset.contains("window.playing = true"),
        "an absolute path to the app's own script was not served, which is \
         a blank screen for every app a bundler built: {asset}. {context}"
    );
    assert!(
        asset.contains("Content-Type: text/javascript"),
        "the app's script was served as something a browser will not run: \
         {asset}. {context}"
    );
    assert!(
        index.contains("<script src=\"webxdc.js\"></script>"),
        "the API was not put in front of the app's page. {context}"
    );
    assert!(
        index.contains("board"),
        "the page served is not the one in the archive. {context}"
    );
    assert!(
        index.contains("Content-Security-Policy: default-src 'self'"),
        "the app was served without a policy confining it. {context}"
    );

    let bridge = value("bridge");
    assert!(
        bridge.starts_with("HTTP/1.1 200 OK") && bridge.contains("self@example.org"),
        "the API was not served with this account's own address in it. {context}"
    );
    assert!(
        !bridge.contains("__SELF_ADDR__"),
        "the API still has its placeholders in it. {context}"
    );

    // Token keeps chat: no token, or wrong one, gets nothing.
    assert!(
        value("api").starts_with("/webxdc-api/") && value("api").len() > 20,
        "the bridge does not carry an unguessable address for the chat: {}. \
         {context}",
        value("api")
    );
    for refused in ["no-token", "wrong-token", "wrong-host", "escape", "missing"] {
        assert!(
            value(refused).starts_with("HTTP/1.1 404"),
            "{refused} was answered rather than refused: {}. {context}",
            value(refused)
        );
    }

    assert!(
        value("send").starts_with("HTTP/1.1 204"),
        "the app could not send an update. {context}"
    );

    assert!(
        value("to-chat").starts_with("HTTP/1.1 204"),
        "the app could not hand a file to the chat: {}. {context}",
        value("to-chat")
    );
    assert!(
        value("to-chat-escape").starts_with("HTTP/1.1 204"),
        "a file named its way out of the cache was refused outright \
         rather than kept under its last name: {}. {context}",
        value("to-chat-escape")
    );
    assert!(
        value("to-chat-big").starts_with("HTTP/1.1 204"),
        "a file the size a shared one actually is did not go through: {}. \
         The head and the body arrive in separate writes, which is what a \
         browser does and what `ask` cannot show. {context}",
        value("to-chat-big")
    );
    assert!(
        value("to-chat-huge").starts_with("HTTP/1.1 204"),
        "a 101 MB file did not go through: {}. There is no size limit on \
         this route: the file is never held, and what used to be a cap \
         was memory this no longer spends. A `write-body:` here is the \
         bug the phone saw -- the host answered and closed while the app \
         was still writing, the app's request died mid-send, and all it \
         could say was that its host could not be reached. {context}",
        value("to-chat-huge")
    );
    assert!(
        value("to-chat-empty").starts_with("HTTP/1.1 400"),
        "a handover with nothing in it was accepted: {}. {context}",
        value("to-chat-empty")
    );

    // What page is told, and what is on disk for it to act on.
    let said = value("log");
    let handed: Vec<&str> = said
        .split(';')
        .filter(|entry| entry.starts_with("to-chat:"))
        .collect();
    assert_eq!(
        handed.len(),
        4,
        "the page was told about {} handovers rather than the four that \
         were accepted: {said:?}. {context}",
        handed.len()
    );
    let video = handed
        .iter()
        .map(|entry| entry.trim_start_matches("to-chat:"))
        .map(|entry| entry.split('|').next().unwrap_or_default())
        .find(|path| path.ends_with("/video.mp4"))
        .unwrap_or_default();
    assert_eq!(
        std::fs::metadata(video).map_or(0, |file| file.len()),
        101 * 1024 * 1024,
        "the 101 MB file did not reach the disk whole: {video:?}. It is \
         written as it arrives, so a short one here is a body the host \
         stopped reading. {context}"
    );
    assert!(
        !stale.exists(),
        "a handover from an earlier run was still in the outbox at {}. \
         Starting the app is what empties it, and without that a copy of \
         every file an app ever handed over stays on the phone. {context}",
        stale.display()
    );
    let first = handed[0].trim_start_matches("to-chat:");
    let (path, words) = first.split_once('|').unwrap_or((first, ""));
    assert_eq!(
        words, "look",
        "the words the app sent with the file did not reach the page: \
         {said:?}. {context}"
    );
    assert!(
        path.ends_with("/notes.txt"),
        "the file was not written under the name the app gave it: \
         {path:?}. {context}"
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap_or_default(),
        "hi\n",
        "the file the page was pointed at does not hold what the app \
         sent. {context}"
    );
    let escaped = handed[1].trim_start_matches("to-chat:");
    let escaped = escaped.split('|').next().unwrap_or_default();
    assert!(
        escaped.ends_with("/passwd") && escaped.contains("/webxdc/outbox/"),
        "a file named `../../../etc/passwd` was written to {escaped:?} \
         rather than under that name inside the cache. {context}"
    );
    let updates = value("updates");
    assert!(
        updates.contains("\"payload\":{\"move\":\"e4\"}") && updates.contains("\"serial\":1"),
        "the update did not come back with a serial on it: {updates}. {context}"
    );
    // Held, and woken by the move, not answered empty.
    assert_eq!(
        value("held-before-send"),
        "true",
        "the request for updates was answered before there was anything \
         to answer it with, so the bridge would be polling. {context}"
    );
    let woken = value("woken");
    assert!(
        woken.starts_with("HTTP/1.1 200")
            && woken.contains("\"payload\":{\"move\":\"e4\"}")
            && woken.contains("\"serial\":1"),
        "the waiting request for updates was not woken with the move: \
         {woken}. {context}"
    );
    // Update changes nothing shown, so no reload: asking core for app info makes it open
    // the archive.
    assert_eq!(
        value("info-calls-after"),
        value("info-calls-before"),
        "the app's info was asked for again on an update. {context}"
    );

    assert_eq!(
        value("stopped-url"),
        "",
        "the app is still being served after it was closed. {context}"
    );
    assert!(
        !value("after-stop").starts_with("HTTP/1.1 200"),
        "a closed app still answers: {}. {context}",
        value("after-stop")
    );
    // Nothing went wrong; app never declared gone. Handovers are in this log;
    // failure must not be.
    let log = value("log");
    assert!(
        !log.contains("error:") && !log.contains("gone;"),
        "the app reported: {log}"
    );

    // Icon came from archive, written where row draws it.
    let icons = cache.join("piirit/piirit/webxdc");
    assert!(
        icons.join("1-5.png").exists(),
        "the app's icon was not cached for its row to draw. {context}"
    );

    let dir = temp.to_path_buf();
    drop(temp);
    assert!(
        !dir.exists(),
        "the test's temp dir, 100 MB video and all, was left behind"
    );
}
