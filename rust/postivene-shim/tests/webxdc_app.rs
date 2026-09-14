//! Running a webxdc app: what the loopback host serves, and to whom.
//!
//! The app is served rather than unpacked, so every file it loads is a
//! request that has to reach the core and come back as the right bytes
//! with the right headers -- and every request that is *not* the app's has
//! to be refused. Both halves are driven here over a real socket, against
//! the fake core, with the same `WebxdcApp` a page instantiates.
//!
//! The round trip is the point of the last part: an update posted by the
//! page reaches the core, the core says so as an event, and the object
//! that started the app has the new summary. That is the whole bridge, in
//! the direction that cannot be checked by reading the JavaScript.
//!
//! `sendToChat` is the other direction and does not go to the core at
//! all: the specification says the reader is asked which chat, so the
//! host writes the file out and raises it on the object for the page to
//! act on. What is pinned here is that a request reaches that signal
//! with a file on the disk to show for it, and that an app cannot use
//! it to write outside the cache.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::io::{Read, Write};
use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Postivene 1.0
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
        // Nothing can be asked of the core until it is up, and it may
        // well be up before this page exists -- which is why the same
        // step runs on the signal and on the way in.
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

/// One request, one answer, one closed connection: what the host does.
/// `Err` is nothing listening, which is what a stopped app should be.
fn ask(authority: &str, request: &str) -> Result<String, String> {
    let mut stream =
        std::net::TcpStream::connect(authority).map_err(|err| format!("connect: {err}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
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

/// A POST whose head and body are separate writes, which is what a real
/// one is: the head goes out, then the body follows in whatever writes the
/// network gives it. `ask` puts both in one, and a host that answers
/// before it has read the body looks fine there and closes the connection
/// under a real client's feet.
///
/// The error is the client's own, because that is the half that matters:
/// a write that fails is what the app sees as a host it could not reach.
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

/// A GET for `path` on the host, as a browser on the phone would send it.
fn get(authority: &str, path: &str) -> Result<String, String> {
    ask(
        authority,
        &format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"),
    )
}

/// The address the app was given. Its files are at the root of it.
fn authority_of(url: &str) -> String {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    rest.split('/').next().unwrap_or_default().to_string()
}

/// Where the chat is, read out of the bridge the app is served.
///
/// The token appears nowhere else -- not in the app's address, not in
/// any page -- which is the point of it: the app is told, and nothing
/// that merely found the port is.
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
    let temp = std::env::temp_dir().join(format!("postivene-webxdc-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let cache = temp.join("cache");
    std::fs::create_dir_all(&cache).expect("create cache dir");

    // A handover from a run that ended before the page could save it.
    // Nothing will ever ask for it again, so starting the app is what
    // clears it: without that, every export a reader ever made would
    // still be on the phone.
    let outbox = cache.join("postivene/postivene/webxdc/outbox/5");
    std::fs::create_dir_all(&outbox).expect("create outbox");
    let stale = outbox.join("stale.bin");
    std::fs::write(&stale, "left over").expect("write a stale handover");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        // The app icon is written here rather than into the runner's own
        // cache, and a fresh directory is what proves it was written.
        std::env::set_var("XDG_CACHE_HOME", &cache);
    }

    postivene_shim::register_qml_types();

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

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and `engine` outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
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
        // The one the phone found: a bundler writes an absolute path into
        // a directory, and a host that keeps the app under a prefix
        // answers nothing to it.
        record!(
            "asset",
            get(&authority, "/assets/app.js").unwrap_or_else(|err| err)
        );
        // The address itself, with nothing after it.
        record!("root", get(&authority, "/").unwrap_or_else(|err| err));
        let bridge = get(&authority, "/webxdc.js").unwrap_or_else(|err| err);
        record!("bridge", bridge.clone());
        let api = api_of(&bridge);
        record!("api", api.clone());
        if api.is_empty() {
            return;
        }

        // Another app on the phone, guessing the port: it can have the
        // files their reader was sent anyway, and nothing of the chat.
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
        // A page in the browser, which would arrive under its own name.
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

        // The app handing a file over, the way `sendToChat` does now:
        // the file *is* the body, and its name is in the query.
        record!(
            "to-chat",
            post_apart(
                &authority,
                &format!("{api}/to-chat?name=notes.txt&text=look"),
                "hi\n"
            )
            .unwrap_or_else(|err| err)
        );

        // A name that tries to leave the cache keeps only its last part.
        record!(
            "to-chat-escape",
            post_apart(
                &authority,
                &format!("{api}/to-chat?name=..%2F..%2F..%2Fetc%2Fpasswd"),
                "hi\n"
            )
            .unwrap_or_else(|err| err)
        );

        // Neither a file nor a word: the app's mistake, and refused.
        record!(
            "to-chat-empty",
            post_apart(&authority, &format!("{api}/to-chat"), "").unwrap_or_else(|err| err)
        );

        // A file the size a shared one actually is, in separate writes
        // the way a browser sends one. Nothing here holds it: it goes
        // from the socket to the cache a chunk at a time.
        let big = "A".repeat(4 * 1024 * 1024);
        record!(
            "to-chat-big",
            post_apart(&authority, &format!("{api}/to-chat?name=photo.png"), &big)
                .unwrap_or_else(|err| err)
        );

        // And one past 100 MB. Nothing holds it either, so the only thing
        // that can stop it is the disk -- a video an app made is exactly
        // the case a cap would be in the way of.
        let huge = "A".repeat(101 * 1024 * 1024);
        record!(
            "to-chat-huge",
            post_apart(&authority, &format!("{api}/to-chat?name=video.mp4"), &huge)
                .unwrap_or_else(|err| err)
        );

        // The app sending a move, the way `webxdc.js` does.
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

    // The event the send produced has had a turn of the loop to arrive,
    // and the object reloads on it.
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("after-update", call!("info"));
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
    // Nothing has been played yet, so the app says nothing about itself.
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
    // What a bundler writes, and what the app on a phone asked for and
    // did not get: an absolute path into a directory. The app is served
    // from the root of its address for this reason.
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

    // The chat is what the token keeps: asking without it, or with the
    // wrong one, is told nothing at all.
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

    // What the page is told, and what is on the disk for it to act on.
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
        std::fs::metadata(video).map(|file| file.len()).unwrap_or(0),
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
    assert_eq!(
        value("after-update"),
        "Checkers|1 move(s)|icon",
        "the app's own summary did not follow the update it sent. {context}"
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
    // Nothing went wrong and the app was never declared gone. The
    // handovers are in this log too and are read out below; what must
    // not be in it is a failure.
    let log = value("log");
    assert!(
        !log.contains("error:") && !log.contains("gone;"),
        "the app reported: {log}"
    );

    // The icon came out of the archive and was written where a row can
    // draw it from.
    let icons = cache.join("postivene/postivene/webxdc");
    assert!(
        icons.join("1-5.png").exists(),
        "the app's icon was not cached for its row to draw. {context}"
    );
}
