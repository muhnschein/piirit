//! What a contact row shows. The contact lists sit one tap from the chat
//! list, so this pins the marks the two share rather than letting them
//! drift into looking like different applications.

// Qt harness: see qml_chat_row.rs.
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
        function load(url) {
            loader.setSource(url, { width: 540 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        function own(property) { return '' + loader.item[property] }
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
    }
";

#[test]
fn a_contact_row_marks_who_can_be_written_to_encrypted() {
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
                QString::from(common::component_url("ContactRow.qml"))
            )
        );
        call!("set", QString::from("displayName"), QString::from("Ada"));
        call!(
            "set",
            QString::from("address"),
            QString::from("ada@example.org")
        );
        call!("set", QString::from("isKeyContact"), true);
        record!(
            "key-name",
            call!("get", QString::from("contactName"), QString::from("text"))
        );
        // Addresses mean nothing to a reader of a chatmail app, so a row
        // shows one only where asked to -- the profiles page, where it
        // is the reader's own.
        record!(
            "address-hidden",
            call!(
                "get",
                QString::from("contactAddress"),
                QString::from("visible")
            )
        );
        call!("set", QString::from("showAddress"), true);
        record!(
            "address",
            call!(
                "get",
                QString::from("contactAddress"),
                QString::from("text")
            )
        );
        // The initial stands in until there is a picture, and it is the
        // shared Avatar doing it.
        record!(
            "initial",
            call!("get", QString::from("avatarInitial"), QString::from("text"))
        );

        // Someone the core cannot encrypt to wears the mail mark. A
        // contact checked in person wears nothing: a tick in a list is a
        // pick, so the row has no way to be told to draw one.
        call!("set", QString::from("isKeyContact"), false);
        record!(
            "plain-name",
            call!("get", QString::from("contactName"), QString::from("text"))
        );
        record!(
            "verified-property",
            call!("own", QString::from("isVerified"))
        );

        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// The marks a row carries, and the ones it must not.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the contact row did not load. {context}"
    );
    assert_eq!(
        value("key-name"),
        "Ada",
        "a contact that can be encrypted to is wearing a mark. {context}"
    );
    assert_eq!(
        value("address-hidden"),
        "false",
        "the row shows the address without being asked to. {context}"
    );
    assert_eq!(
        value("address"),
        "ada@example.org",
        "the row does not show the address where it is asked to. {context}"
    );
    assert_eq!(
        value("initial"),
        "A",
        "the avatar is not standing in with an initial. {context}"
    );
    assert_eq!(
        value("plain-name"),
        "✉ Ada",
        "a contact that cannot be encrypted to is not marked, so it looks \
         the same as one that can. {context}"
    );
    assert_eq!(
        value("verified-property"),
        "undefined",
        "the row can be told a contact was checked in person, which it \
         would draw as a tick -- the mark a picker uses for a pick. {context}"
    );
}
