//! Map app (core's webxdc integration) served: its tiles come through core, nothing else does.
//!
//! App asks OSM's tile server itself; host rewrites that address in its scripts to own path
//! behind chat token, fetches via core's `get_http_response`. Only digits reach core; only a
//! picture reaches page. Ordinary app: no tile path, scripts untouched.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property int mapId: 0
        property string mapQuery: ''
        property int empties: 0
        property string failed: ''
        LocationMap {
            id: map
            account_id: 1
            onOpened: { mapId = message_id; mapQuery = query }
            onEmpty: empties++
            onError: failed = message
        }
        WebxdcApp { id: app; account_id: 1; onError: failed = message }
        // Nothing asked of core until it is up. Chat 2 has no locations, 1 has.
        function open(chat) {
            if (core.status !== 'ready') {
                return 'waiting'
            }
            map.open(chat, 0, 0)
            return 'ok'
        }
        function emptied() { return '' + empties }
        function query() { return mapQuery }
        // Map app, else ordinary app 5.
        function start(useMap) {
            app.stop()
            app.message_id = useMap ? mapId : 5
            app.start()
            return 'ok'
        }
        function opened() { return '' + mapId }
        function url() { return app.url }
        function error() { return failed }
    }
";

/// One GET, whole answer; `Err` if host unreachable.
fn get(authority: &str, path: &str) -> String {
    let answer = (|| -> Result<String, String> {
        let mut stream =
            std::net::TcpStream::connect(authority).map_err(|err| format!("connect: {err}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|err| err.to_string())?;
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
        stream
            .write_all(request.as_bytes())
            .map_err(|err| format!("write: {err}"))?;
        let mut answer = Vec::new();
        stream
            .read_to_end(&mut answer)
            .map_err(|err| format!("read: {err}"))?;
        Ok(String::from_utf8_lossy(&answer).into_owned())
    })();
    answer.unwrap_or_else(|err| err)
}

/// Status line and body, headers dropped but content type.
fn summary(answer: &str) -> String {
    let status = answer.lines().next().unwrap_or_default().to_string();
    let kind = answer
        .lines()
        .find_map(|line| line.strip_prefix("Content-Type: "))
        .unwrap_or_default();
    let body = answer.split("\r\n\r\n").nth(1).unwrap_or_default();
    format!("{status}|{kind}|{body}")
}

/// Map points' labels from an updates answer, `|`-joined.
fn labels(answer: &str) -> String {
    let body = answer.split("\r\n\r\n").nth(1).unwrap_or_default();
    let list: Vec<Value> = serde_json::from_str(body).unwrap_or_default();
    list.iter()
        .map(|update| {
            update
                .pointer("/payload/label")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn authority_of(url: &str) -> String {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    rest.split('/').next().unwrap_or_default().to_string()
}

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
fn a_map_gets_its_tiles_through_core_and_nothing_else() {
    let temp = common::temp_tree("webxdc-map");
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("XDG_CACHE_HOME", temp.join("cache"));
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
    let steps: common::Steps = Rc::new(RefCell::new(Vec::new()));

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = unsafe {
                (*engine_ptr).invoke_method($name.into(), &[$(QVariant::from($arg)),*])
            };
            QString::from_qvariant(result).unwrap_or_default().to_string()
        }};
    }

    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    plan.until(1, 10, move |_| call!("open", 2) == "ok");
    // Nothing to show: no app readied.
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("emptied") == "1";
        if done || last {
            common::record(&s, "empty-chat", call!("emptied").into());
            common::record(&s, "empty-opened", call!("opened").into());
            call!("open", 1);
        }
        done
    });
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let done = call!("opened") != "0";
        if done || last {
            common::record(&s, "query", call!("query").into());
            call!("start", true);
        }
        done
    });

    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let url = call!("url");
        let done = !url.is_empty();
        if done || last {
            let authority = authority_of(&url);
            let bridge = get(&authority, "/webxdc.js");
            let api = api_of(&bridge);
            let record = |label: &str, value: String| common::record(&s, label, value.into());
            record("api", api.clone());
            record("view", bridge.contains("map.zoom").to_string());
            record(
                "points",
                labels(&get(&authority, &format!("{api}/updates?serial=0"))),
            );
            record("script", summary(&get(&authority, "/index.js")));
            // Not a script: left as is.
            record("text", summary(&get(&authority, "/notes.txt")));
            record(
                "tile",
                summary(&get(&authority, &format!("{api}/tile/3/4/5.png"))),
            );
            // Server's error page, not a picture: page never gets it.
            record(
                "blocked",
                summary(&get(&authority, &format!("{api}/tile/7/1/1.png"))),
            );
            for (label, path) in [
                ("jpg", "/tile/3/4/5.jpg"),
                ("short", "/tile/3/4.png"),
                ("long", "/tile/3/4/5/6.png"),
                ("letters", "/tile/a/4/5.png"),
                ("empty", "/tile//4/5.png"),
                ("huge", "/tile/12345678/4/5.png"),
                ("escape", "/tile/../../3/4/5.png"),
            ] {
                record(label, summary(&get(&authority, &format!("{api}{path}"))));
            }
            // Port finder: no token, no tiles.
            record("no-token", summary(&get(&authority, "/tile/3/4/5.png")));
            call!("start", false);
        }
        done
    });

    // Ordinary app: no tiles, though asked with its token.
    let s = steps.clone();
    plan.until(1, 10, move |last| {
        let url = call!("url");
        let done = !url.is_empty() && call!("error").is_empty();
        if done || last {
            let authority = authority_of(&url);
            let bridge = get(&authority, "/webxdc.js");
            let api = api_of(&bridge);
            common::record(
                &s,
                "other-view",
                bridge.contains("map.zoom").to_string().into(),
            );
            common::record(
                &s,
                "other-tile",
                summary(&get(&authority, &format!("{api}/tile/3/4/5.png"))).into(),
            );
            unsafe { (*engine_ptr).quit() };
        }
        done
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");
    let api = value("api");
    assert!(api.starts_with("/webxdc-api/"), "{context}");
    assert_eq!(
        value("empty-chat"),
        "1",
        "a chat with no locations readied a map. {context}"
    );
    assert_eq!(value("empty-opened"), "0", "{context}");
    // Chat 1: Ada's points at 60.1/24.9 and 60.2/25.0 (fake core).
    assert_eq!(value("query"), "?bounds=60.1,24.9,60.2,25", "{context}");
    assert_eq!(
        value("view"),
        "true",
        "map opens without framing its points. {context}"
    );
    assert_eq!(value("other-view"), "false", "{context}");
    assert_eq!(
        value("points"),
        "Ada|Café|",
        "a place labelled only by its link was not named after its sender. {context}"
    );

    assert_eq!(
        value("script"),
        format!(
            "HTTP/1.1 200 OK|text/javascript; charset=utf-8|\
             let url = '{api}/tile/{{z}}/{{x}}/{{y}}.png';\n"
        ),
        "map's tile address not turned to host's own. {context}"
    );
    assert_eq!(
        value("text"),
        "HTTP/1.1 200 OK|text/plain; charset=utf-8|https://tile.openstreetmap.org/ stays\n",
        "{context}"
    );
    assert_eq!(
        value("tile"),
        "HTTP/1.1 200 OK|image/png|\u{89}PNG tile 3/4/5.png",
        "tile not handed through from core. {context}"
    );
    assert_eq!(
        value("blocked"),
        "HTTP/1.1 502 Bad Gateway|text/plain; charset=utf-8|502 Bad Gateway",
        "server's error page reached the map as a tile. {context}"
    );
    for label in [
        "jpg",
        "short",
        "long",
        "letters",
        "empty",
        "huge",
        "escape",
        "no-token",
        "other-tile",
    ] {
        assert!(
            value(label).starts_with("HTTP/1.1 404 Not Found|"),
            "{label} not refused. {context}"
        );
    }

    // Only well-formed tile addresses reach core.
    let asked: Vec<Value> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "get_http_response")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        asked,
        [
            serde_json::json!([1, "https://tile.openstreetmap.org/3/4/5.png"]),
            serde_json::json!([1, "https://tile.openstreetmap.org/7/1/1.png"]),
        ],
        "core asked for something other than the two tiles"
    );
}
