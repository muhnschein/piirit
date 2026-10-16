//! Contact request's bar, in the message field's place: says why, offers turn-down and Accept.
//!
//! One-to-one turns down with Block (sender named), group with Delete. Bar only reports taps;
//! page acts (`chat_contact_request.rs` covers model side). Hidden bar takes no room.

// Qt harness: `unsafe` env::set_var pre-Qt (`unused_unsafe`: unsafe only from edition 2024),
// `borrow_as_ptr` for engine pointer, `single_shot` with whole-second Durations.
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
    Item {
        Loader { id: loader }
        property string heard: ''
        function load(url) {
            loader.setSource(url, { width: 540, chatName: 'Mira Kallio', shown: true })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.acceptClicked.connect(function () { heard += 'accept;' })
            loader.item.declineClicked.connect(function () { heard += 'decline;' })
            return 'ok'
        }
        function set(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function own(property) { return '' + loader.item[property] }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function heardSoFar() { return heard }
    }
";

#[test]
fn a_request_bar_names_who_wrote_and_offers_the_right_turn_down() {
    // SAFETY: single-threaded; set pre-Qt.
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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("RequestBar.qml"))
            )
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("single-text", get!("requestText", "text"));
        record!("single-decline", get!("declineButton", "text"));
        record!("accept", get!("acceptButton", "text"));
        record!("height", call!("own", QString::from("height")));
        record!("tap-decline", call!("tap", QString::from("declineButton")));
        record!("tap-accept", call!("tap", QString::from("acceptButton")));
        call!("set", QString::from("isGroup"), true);
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("group-text", get!("requestText", "text"));
        record!("group-decline", get!("declineButton", "text"));
        record!("heard", call!("heardSoFar"));
        call!("set", QString::from("shown"), false);
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("hidden-height", call!("own", QString::from("height")));
        record!("hidden", call!("own", QString::from("visible")));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// What the bar says and reports.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the bar did not load. {context}");
    assert!(
        value("single-text").starts_with("Mira Kallio isn't in your contacts"),
        "a one-to-one request does not say who wrote. {context}"
    );
    assert!(
        value("single-text").contains("won't be told that you've read"),
        "the bar does not say reading sends no read receipt. {context}"
    );
    assert_eq!(
        value("single-decline"),
        "Block",
        "a one-to-one request is not turned down by blocking. {context}"
    );
    assert_eq!(value("accept"), "Accept", "no way to accept. {context}");
    assert!(
        value("height").parse::<f64>().unwrap_or_default() > 0.0,
        "the shown bar takes no room. {context}"
    );
    assert_eq!(
        value("group-decline"),
        "Delete",
        "a group request offers blocking a whole group. {context}"
    );
    assert!(
        value("group-text").contains("added you to this group"),
        "a group request does not say the reader was added. {context}"
    );
    assert_eq!(
        value("heard"),
        "decline;accept;",
        "the bar's buttons do not report their taps. {context}"
    );
    assert_eq!(
        value("hidden"),
        "false",
        "the bar stays up once the request is answered. {context}"
    );
    assert_eq!(
        value("hidden-height"),
        "0",
        "the hidden bar still pushes the message field up. {context}"
    );
}
