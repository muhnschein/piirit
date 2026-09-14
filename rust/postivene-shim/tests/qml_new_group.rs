//! Making a group: named, pictured and peopled on one page, and handed to
//! the core in one go.
//!
//! The page mirrors the group's own (`qml_group_page.rs`), so what is
//! checked is the same shape: the reader among the members from the
//! start, whoever the picker hands back drawn as a member, and creating
//! refused until there is a name -- which needs `nameField` to be reachable
//! from the page's own scope, not buried in the list's header where the
//! Create button cannot see it.
//!
//! "Drawn as a member" is asked as `shown`, not as `visible`: the rows
//! are the ones `picked_rows` hands back, so a contact nobody picked has
//! no row at all rather than a hidden one.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

// Silica's `pageStack`, recorded rather than performed -- in QML rather
// than as a QObject of the test's own. The page resolves the bare name
// through the component that instantiated it, which is this one, so a
// property of the root suffices; and a QObject derived here would carry
// the generated dispatcher's pointer juggling into a file that only
// reads back.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property QtObject pageStack: QtObject {
            /// `push:Foo.qml|replace:Bar.qml|...`
            property string log: ''
            /// The chat the conversation was opened on.
            property int chatId: 0
            function note(verb, page, properties) {
                var name = ('' + page).split('/').pop()
                log = log + verb + ':' + name + '|'
                if (properties && properties.chatId !== undefined) {
                    chatId = properties.chatId
                }
            }
            function push(page, properties) { note('push', page, properties) }
            function replace(page, properties) { note('replace', page, properties) }
        }
        Loader { id: loader }
        function load(url, accountId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
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
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
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
        // Drawn as a member, or not drawn at all: the page asks the
        // contact list for the rows it wants rather than building one
        // per contact and hiding all but the members, so a contact
        // nobody picked has no row rather than a hidden one.
        function shown(name) {
            var item = findIn(loader.item, name)
            return item ? '' + item.visible : 'false'
        }
        // What the picker page does with the stand-in it was handed: the
        // page's own answer for a group that does not exist yet.
        function pick(contactId) {
            loader.item.pendingGroup.add_members([contactId])
            return '' + loader.item.pendingGroup.is_member(contactId)
        }
        function unpick(contactId) {
            loader.item.removeMember(contactId)
            return '' + loader.item.pendingGroup.is_member(contactId)
        }
        // What the gallery page reports back, without the gallery.
        function choosePicture(path) {
            loader.item.picturePath = path
            return 'ok'
        }
        function navigation() { return pageStack.log }
        function openedChat() { return '' + pageStack.chatId }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_new_group_page_makes_the_group_it_shows() {
    let temp = std::env::temp_dir().join(format!("postivene-new-group-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("NewGroupPage.qml")),
                1
            )
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        // Nothing typed: creating must be refused.
        record!("empty", get!("createButton", "enabled"));
        // The reader is a member from the start; nobody else is yet.
        record!("self-row", call!("shown", QString::from("memberRow1")));
        record!("ada-before", call!("shown", QString::from("memberRow10")));
        record!("heading-before", get!("membersHeader", "text"));
        record!("badge", get!("editBadge", "visible"));
        record!("no-picture-yet", get!("removePicture", "visible"));
        record!(
            "typed",
            call!(
                "setText",
                QString::from("nameField"),
                QString::from("Walking group")
            )
        );
        // The picker hands Ada back; the same again is not twice.
        record!("pick-ada", call!("pick", 10));
        record!("pick-ada-again", call!("pick", 10));
        record!("ada-after", call!("shown", QString::from("memberRow10")));
        record!("heading-after", get!("membersHeader", "text"));
        // Grace comes and goes again.
        record!("pick-grace", call!("pick", 11));
        record!("unpick-grace", call!("unpick", 11));
        record!("grace-after", call!("shown", QString::from("memberRow11")));
        record!(
            "picture",
            call!("choosePicture", QString::from("/tmp/hikers.png"))
        );
        record!("picture-offered", get!("removePicture", "visible"));
        record!("avatar-picture", get!("groupAvatar", "picturePath"));
        record!("add-members-row", get!("addMembersButton", "visible"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("named", get!("createButton", "enabled"));
        record!("create", call!("click", QString::from("createButton")));
        record!("creating", get!("createButton", "enabled"));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("navigation", call!("navigation"));
        record!("opened", call!("openedChat"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps, &common::calls(&journal));
}

/// The page refused an unnamed group, showed the members it was handed,
/// and made the group with its name, its members and its picture.
#[allow(clippy::too_many_lines)]
fn assert_outcome(steps: &[(&str, String)], calls: &[(String, Value)]) {
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}\ncalls: {names:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let navigation = value("navigation");
    let chat_id: u32 = value("opened").parse().unwrap_or(0);

    for label in ["load", "typed", "picture", "create"] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert_eq!(
        value("empty"),
        "false",
        "creating was offered with nothing typed. {context}"
    );
    assert_eq!(
        value("self-row"),
        "true",
        "the reader is not listed among the members of the group they \
         are making. {context}"
    );
    assert_eq!(
        value("ada-before"),
        "false",
        "a contact nobody picked is drawn as a member. {context}"
    );
    assert!(
        value("heading-before").starts_with('1'),
        "the heading does not count the reader alone before anyone is \
         picked. {context}"
    );
    assert_eq!(
        value("badge"),
        "true",
        "the picture's edit badge is not offered on a group being made. {context}"
    );
    assert_eq!(
        value("no-picture-yet"),
        "false",
        "removing a picture is offered before one was chosen. {context}"
    );
    for label in ["pick-ada", "pick-ada-again", "pick-grace"] {
        assert_eq!(
            value(label),
            "true",
            "the stand-in for the group did not take a member. {context}"
        );
    }
    assert_eq!(
        value("ada-after"),
        "true",
        "a picked contact is not drawn as a member. {context}"
    );
    assert!(
        value("heading-after").starts_with('2'),
        "the heading does not count the reader and the one picked. {context}"
    );
    assert_eq!(
        value("unpick-grace"),
        "false",
        "a member taken off the list is still counted in. {context}"
    );
    assert_eq!(
        value("grace-after"),
        "false",
        "a member taken off the list is still drawn. {context}"
    );
    assert_eq!(
        value("picture-offered"),
        "true",
        "a chosen picture cannot be removed again. {context}"
    );
    assert_eq!(
        value("avatar-picture"),
        "/tmp/hikers.png",
        "the chosen picture is not the one the avatar shows. {context}"
    );
    assert_eq!(
        value("add-members-row"),
        "true",
        "the way to more members is missing from the end of the list. {context}"
    );
    assert_eq!(
        value("named"),
        "true",
        "typing a name did not enable creating the group. {context}"
    );
    assert_eq!(
        value("creating"),
        "false",
        "a second tap on create is still offered while the first is being \
         answered. {context}"
    );

    let created: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "create_group_chat")
        .map(|(_, params)| params.clone())
        .collect();
    assert_eq!(
        created,
        vec![serde_json::json!([1, "Walking group", false])],
        "the group was not made once, encrypted, with the typed name. {context}"
    );
    assert_ne!(
        chat_id, 0,
        "the conversation was opened on no chat. {context}"
    );
    let added: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "add_contact_to_chat")
        .map(|(_, params)| params.clone())
        .collect();
    assert_eq!(
        added,
        vec![serde_json::json!([1, chat_id, 10])],
        "the members added are not the one picked and kept, on the group \
         just made. {context}"
    );
    let pictured: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "set_chat_profile_image")
        .map(|(_, params)| params.clone())
        .collect();
    assert_eq!(
        pictured,
        vec![serde_json::json!([1, chat_id, "/tmp/hikers.png"])],
        "the chosen picture was not put on the group once it existed. {context}"
    );
    assert!(
        navigation.ends_with("replace:ConversationPage.qml|"),
        "making the group did not open it in place of this page. {context}"
    );
}
