//! Tap on contact/group picture opens it larger. Ada has picture
//! (`PIIRIT_FAKE_CONTACT_PICTURE`): opens on that file. Group has none: tap opens
//! nothing until pull-down sets one. Changing group picture is in pull-down; no edit badge on
//! picture.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Ada's picture: space and `#` in name, kept only by per-segment encoding.
const ADA_PICTURE: &str = "/tmp/piirit-blobs/ada #1.jpg";
const ADA_URL: &str = "file:///tmp/piirit-blobs/ada%20%231.jpg";
const GROUP_PICTURE: &str = "/tmp/piirit-blobs/hikers.png";
const GROUP_URL: &str = "file:///tmp/piirit-blobs/hikers.png";

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        // Silica's `pageStack`, recorded rather than performed: which page
        // each tap opens, and on which file. The pages find it by id, up
        // the loader's context.
        QtObject {
            id: pageStack
            /// `push:PicturePage.qml=file:///...|push:AttachPhotoPage.qml=|`
            property string log: ''
            function push(page, properties) {
                var name = String(page).split('/').pop()
                var file = properties && properties.fileUrl ? String(properties.fileUrl) : ''
                log += 'push:' + name + '=' + file + '|'
                return null
            }
        }
        function load(url, chatId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: 1, chatId: chatId, chatName: 'from the list' })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // `data` rather than `children`: the model is a plain QObject, so
        // it is not among an Item's visual children at all.
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
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        // A tap on a MouseArea: its clicked() carries the event, and QML
        // refuses to emit it without one. The signal itself, whether or
        // not the area is enabled: a tap that reaches the page anyway
        // must still open nothing when there is nothing to show.
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked(null)
            return 'ok'
        }
        // What the page stack has been asked for since the last look.
        function pushed() {
            var log = pageStack.log
            pageStack.log = ''
            return log
        }
        // What the gallery hands back once a picture is picked.
        function pick(path) {
            var chat = findIn(loader.item, 'chat')
            if (!chat) { return 'missing:chat' }
            chat.set_picture(path)
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_tap_on_the_picture_shows_it_larger() {
    let temp = std::env::temp_dir().join(format!("piirit-info-picture-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_CONTACT_PICTURE", ADA_PICTURE);
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

    // Ada: chat 1, one member with picture.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load-contact",
            call!(
                "load",
                QString::from(common::page_url("ContactPage.qml")),
                1
            )
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("contact-picture", get!("contactAvatar", "picturePath"));
        record!("contact-tappable", get!("pictureTap", "enabled"));
        record!("contact-tap", call!("tap", QString::from("pictureTap")));
        record!("contact-opened", call!("pushed"));
        // Group: chat 2, no picture yet.
        record!(
            "load-group",
            call!("load", QString::from(common::page_url("GroupPage.qml")), 2)
        );
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("group-loaded", get!("chat", "loaded"));
        record!("group-badge", get!("editBadge", "visible"));
        record!("group-tappable", get!("pictureTap", "enabled"));
        record!("group-tap", call!("tap", QString::from("pictureTap")));
        record!("group-opened-bare", call!("pushed"));
        // Picture change in pull-down, opens gallery.
        record!("change-offered", get!("changePicture", "visible"));
        record!("change", call!("click", QString::from("changePicture")));
        record!("change-opened", call!("pushed"));
        record!("pick", call!("pick", QString::from(GROUP_PICTURE)));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("group-picture", get!("groupAvatar", "picturePath"));
        record!("group-tappable-now", get!("pictureTap", "enabled"));
        record!("group-tap-now", call!("tap", QString::from("pictureTap")));
        record!("group-opened", call!("pushed"));
        // One-to-one chat has nothing to change.
        record!(
            "load-single",
            call!("load", QString::from(common::page_url("GroupPage.qml")), 1)
        );
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("single-loaded", get!("chat", "loaded"));
        record!("single-change", get!("changePicture", "visible"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_taps(&steps);
}

fn assert_taps(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    for label in [
        "load-contact",
        "contact-tap",
        "load-group",
        "group-tap",
        "change",
        "pick",
        "group-tap-now",
        "load-single",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }

    // Ada's picture: shown, tappable, opened large.
    assert_eq!(
        value("contact-picture"),
        ADA_PICTURE,
        "the contact page does not show Ada's picture. {context}"
    );
    assert_eq!(
        value("contact-tappable"),
        "true",
        "a contact's picture cannot be tapped. {context}"
    );
    assert_eq!(
        value("contact-opened"),
        format!("push:PicturePage.qml={ADA_URL}|"),
        "a tap on a contact's picture does not open it large. {context}"
    );

    // Group, no picture: nothing to tap or open, no badge.
    assert_eq!(
        value("group-loaded"),
        "true",
        "the group never loaded. {context}"
    );
    assert_eq!(
        value("group-badge"),
        "missing:editBadge",
        "the group's picture still wears an edit badge. {context}"
    );
    assert_eq!(
        value("group-tappable"),
        "false",
        "a group with no picture offers a tap on its disc. {context}"
    );
    assert_eq!(
        value("group-opened-bare"),
        "",
        "a tap on a group with no picture opened something. {context}"
    );

    // Changing: pull-down, to gallery.
    assert_eq!(
        value("change-offered"),
        "true",
        "the pull-down does not offer to change the group's picture. {context}"
    );
    assert_eq!(
        value("change-opened"),
        "push:AttachPhotoPage.qml=|",
        "changing the picture does not open the gallery. {context}"
    );

    // Once set, tap shows it large, no gallery again.
    assert_eq!(
        value("group-picture"),
        GROUP_PICTURE,
        "the picked picture did not reach the group. {context}"
    );
    assert_eq!(
        value("group-tappable-now"),
        "true",
        "a group's picture cannot be tapped. {context}"
    );
    assert_eq!(
        value("group-opened"),
        format!("push:PicturePage.qml={GROUP_URL}|"),
        "a tap on a group's picture does not open it large. {context}"
    );

    assert_eq!(
        value("single-loaded"),
        "true",
        "the one-to-one chat never loaded. {context}"
    );
    assert_eq!(
        value("single-change"),
        "false",
        "a chat that is not a group offers to change its picture. {context}"
    );
}
