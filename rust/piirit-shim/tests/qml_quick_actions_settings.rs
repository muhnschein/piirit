//! Cover quick actions in settings: one row under Advanced, opening their page. That page:
//! what they are, two cover pictures (room for one or two), then what each does.
//!
//! Each writes its choice and shows what settings hold; pictures redraw. A chat is picked on
//! the chat picker; the action is a chat's only once picked, named by core's current name,
//! icon choosable, and says so if deleted since. Chat may be any profile's: picker opens on
//! the action's chat's profile; chat is whichever profile's it was picked from. Camera
//! actions only: a switch says whether the item is sent at once; off unless turned on
//! (asks first), off again whenever an action is chosen; per side. Room for one: right
//! action hidden and kept.

// Qt harness: see qml_pages.rs.
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

/// Probe, components dir filled in; see `qml_general_settings.rs`.
fn probe_qml() -> String {
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    import 'file://__COMPONENTS__'
    Item {
        id: probe

        // The page stack, handing back a picker the probe can answer for.
        property QtObject pageStack: QtObject {
            property string log: ''
            // Silica's page of choices going, or any other move.
            property bool busy: false
            // Opens on the profile it is handed, as the page does, and
            // says whether it offers the others.
            property QtObject picker: QtObject {
                property int accountId: 0
                signal chatPicked(int chatId, string chatName)
            }
            function name(page) { return ('' + page).split('/').pop() }
            // What turning on a switch worth stopping for asks.
            property QtObject dialog: QtObject {
                property string text: ''
                signal accepted()
            }
            function push(page, properties) {
                log += 'push:' + name(page) + ':' + properties.accountId + ':'
                       + properties.title + ':' + properties.profileChoice + '|'
                if (name(page) === 'ExperimentalFeatureDialog.qml') {
                    dialog.text = properties.text
                    return dialog
                }
                picker.accountId = properties.accountId || 0
                return picker
            }
        }

        // For deleting a chat out from under an action.
        ChatList { id: helper; account_id: 1 }

        Loader { id: loader }
        function load(url) {
            // The profiles there are, which the app has read long before
            // anyone reaches its settings.
            core.refresh_accounts()
            loader.setSource(url, { accountId: 1 })
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
            if (node.menu) {
                var inMenu = findIn(node.menu, name)
                if (inMenu) { return inMenu }
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
        // A kind tapped on Silica's ComboBox, which moves the choice to
        // it, and writes its text in as the value, before it reports the
        // tap.
        function tapKind(side, kind) {
            var combo = findIn(loader.item, side + 'ActionChoice')
            var item = findIn(loader.item, side + 'Action-' + (kind || 'none'))
            if (!combo || !item) { return 'missing:' + side + ':' + kind }
            combo.currentIndex = kinds.indexOf(kind)
            combo.value = item.text
            item.clicked()
            return 'ok'
        }
        readonly property var kinds: ['', 'chat', 'search', 'qr', 'scan', 'profiles',
                                      'photo', 'video', 'selfie', 'selfie-video']
        // What the choice is on, by kind.
        function onKind(side) {
            var combo = findIn(loader.item, side + 'ActionChoice')
            return combo ? (kinds[combo.currentIndex] || 'none') : 'missing'
        }
        function accept() { pageStack.dialog.accepted(); return 'ok' }
        function warning() { return pageStack.dialog.text }
        function setBusy(busy) { pageStack.busy = busy === 'true'; return 'ok' }
        // The page under Silica's page of choices, or back on screen.
        function setActive(active) {
            loader.item.status = active === 'true' ? PageStatus.Active : PageStatus.Inactive
            return 'ok'
        }
        // What the settings hold for one side, as kind|account|chat|icon.
        function count() { return '' + Settings.quickActionCount }
        function holds(side) {
            var prefix = side === 'right' ? 'quickActionRight' : 'quickActionLeft'
            return Settings[prefix] + '|' + Settings[prefix + 'Account'] + '|'
                   + Settings[prefix + 'Chat'] + '|' + Settings[prefix + 'Icon']
        }
        // The icon a picture of the cover shows in an action's place: the
        // file's name, or '' for the dot of one not chosen yet.
        function previewIcon(preview, index) {
            var picture = findIn(loader.item, preview)
            if (!picture) { return 'missing:' + preview }
            var spot = findIn(picture, 'previewAction' + index)
            if (!spot) { return 'missing:previewAction' + index }
            return ('' + spot.source).split('/').pop()
        }
        // Whether a side sends what its camera takes at once.
        function sendsNow(side) {
            return '' + Settings[side === 'right' ? 'quickActionRightSendNow'
                                                  : 'quickActionLeftSendNow']
        }
        function clear() {
            Settings.quickActionLeftSendNow = false
            Settings.quickActionRightSendNow = false
            Settings.quickActionCount = 1
            Settings.quickActionLeft = ''
            Settings.quickActionLeftAccount = 0
            Settings.quickActionLeftChat = 0
            Settings.quickActionLeftIcon = ''
            Settings.quickActionRight = ''
            Settings.quickActionRightAccount = 0
            Settings.quickActionRightChat = 0
            Settings.quickActionRightIcon = ''
            return 'ok'
        }
        function pick(chatId, chatName) {
            pageStack.picker.chatPicked(chatId, chatName)
            return 'ok'
        }
        // The reader turns the picker to another profile first.
        function pickIn(accountId, chatId, chatName) {
            pageStack.picker.accountId = accountId
            pageStack.picker.chatPicked(chatId, chatName)
            return 'ok'
        }
        // An action whose chat is in a profile deleted since.
        function orphan(side, account) {
            var prefix = side === 'right' ? 'quickActionRight' : 'quickActionLeft'
            Settings[prefix] = 'chat'
            Settings[prefix + 'Account'] = account
            Settings[prefix + 'Chat'] = 1
            return 'ok'
        }
        function stackLog() {
            var log = pageStack.log
            pageStack.log = ''
            return log
        }
        function deleteChat(chatId) { helper.delete_chat(chatId); return 'ok' }
        // What stands after an item in the page's column: its objectName,
        // or the text of a heading.
        function after(name) {
            var item = findIn(loader.item, name)
            return item ? describe(next(item)) : 'missing:' + name
        }
        function underHeading(heading) {
            var header = findText(loader.item, heading)
            return header ? describe(next(header)) : 'missing:' + heading
        }
        function next(item) {
            var kids = item.parent.children
            for (var i = 0; i + 1 < kids.length; i++) {
                if (kids[i] === item) { return kids[i + 1] }
            }
            return null
        }
        function describe(item) {
            if (!item) { return 'nothing' }
            return item.objectName !== '' ? item.objectName : 'heading:' + item.text
        }
        function findText(node, text) {
            if (!node) { return null }
            if (node.text === text && node.objectName === '') { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findText(kids[i], text)
                if (hit) { return hit }
            }
            return null
        }
    }
";

/// Camera actions: kind, what the choice says for it, and recording step.
const CAMERA_KINDS: [(&str, &str, &str); 4] = [
    ("photo", "Photo", "camera-photo"),
    ("video", "Video", "camera-video"),
    ("selfie", "Selfie", "camera-selfie"),
    ("selfie-video", "Selfie video", "camera-selfie-video"),
];

#[test]
#[allow(clippy::too_many_lines)]
fn the_quick_actions_are_set_up_on_a_page_of_their_own() {
    let temp =
        std::env::temp_dir().join(format!("piirit-qml-quick-settings-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server start.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Two profiles: an action's chat can be the other one's.
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml().as_str()));

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
    macro_rules! click {
        ($name:expr) => {
            call!("click", QString::from($name))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    // SAFETY: callbacks run only inside `exec()` on this thread.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!("clear", call!("clear"));
        record!(
            "load-settings",
            call!("load", QString::from(common::page_url("SettingsPage.qml")))
        );

        // Settings page: one row under Advanced, then webxdc and calls switches; nothing after.
        record!(
            "under-advanced",
            call!("underHeading", QString::from("Advanced"))
        );
        record!(
            "after-entry",
            call!("after", QString::from("quickActionsEntry"))
        );
        record!(
            "after-webxdc",
            call!("after", QString::from("webxdcSwitch"))
        );
        record!(
            "old-heading",
            call!("underHeading", QString::from("Quick actions"))
        );
        record!("apps-heading", call!("underHeading", QString::from("Apps")));
        record!("open", click!("quickActionsEntry"));
        record!("opened", call!("stackLog"));

        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("QuickActionsPage.qml"))
            )
        );

        // Their page: word on what they are, pictures, then actions.
        record!(
            "after-header",
            call!("after", QString::from("quickActionsHeader"))
        );
        record!(
            "after-words",
            call!("after", QString::from("quickActionsExplained"))
        );
        record!(
            "after-choices",
            call!("after", QString::from("quickActionCountChoices"))
        );
        record!(
            "after-left",
            call!("after", QString::from("leftQuickAction"))
        );
        record!("words", get!("quickActionsExplained", "text"));

        // Never-asked phone: room for one, that one none.
        record!("one-chosen", get!("oneActionPreview", "selected"));
        record!("two-unchosen", get!("twoActionsPreview", "selected"));
        record!("one-label", get!("leftActionChoice", "label"));
        record!("right-hidden", get!("rightQuickAction", "visible"));
        record!("left-none-says", get!("leftActionChoice", "value"));
        record!("left-none-on", call!("onKind", QString::from("left")));
        record!("icons-hidden", get!("leftActionIcons", "visible"));
        record!(
            "one-dot",
            call!("previewIcon", QString::from("oneActionPreview"), 0)
        );

        // Kinds needing nothing more are written on tap; pictures follow.
        record!("pick-search", click!("leftAction-search"));
        record!("left-search", call!("holds", QString::from("left")));
        record!("left-search-says", get!("leftActionChoice", "value"));
        record!(
            "one-search",
            call!("previewIcon", QString::from("oneActionPreview"), 0)
        );
        record!(
            "two-left-search",
            call!("previewIcon", QString::from("twoActionsPreview"), 0)
        );
        record!(
            "two-right-dot",
            call!("previewIcon", QString::from("twoActionsPreview"), 1)
        );

        // Room for two: other picture chosen, left and right.
        record!("choose-two", click!("twoActionsPreview"));
        record!("count-two", call!("count"));
        record!("two-chosen", get!("twoActionsPreview", "selected"));
        record!("one-unchosen", get!("oneActionPreview", "selected"));
        record!("left-label", get!("leftActionChoice", "label"));
        record!("right-shown", get!("rightQuickAction", "visible"));
        record!("pick-qr", click!("rightAction-qr"));
        record!("right-qr-shown", get!("rightActionChoice", "value"));
        record!(
            "two-right-qr",
            call!("previewIcon", QString::from("twoActionsPreview"), 1)
        );
        record!("pick-scan", click!("rightAction-scan"));
        record!("right-scan", call!("holds", QString::from("right")));
        record!("right-scan-shown", get!("rightActionChoice", "value"));
        record!("pick-none", click!("rightAction-none"));
        record!("right-none-again", call!("holds", QString::from("right")));
        record!("right-none-shown", get!("rightActionChoice", "value"));
        // Profiles, with own icon.
        record!("pick-profiles", click!("rightAction-profiles"));
        record!("right-profiles", call!("holds", QString::from("right")));
        record!("right-profiles-shown", get!("rightActionChoice", "value"));
        record!(
            "two-right-profiles",
            call!("previewIcon", QString::from("twoActionsPreview"), 1)
        );
        // No send-at-once switch under an action taking nothing.
        record!("qr-switch", get!("rightActionSendNow", "visible"));
        // Camera's four, each written on tap with its switch, off until turned on.
        for (kind, label) in CAMERA_KINDS.map(|(kind, _, label)| (kind, label)) {
            let item = format!("rightAction-{kind}");
            let held = call!("click", QString::from(item.as_str()));
            let held = format!(
                "{held}|{}|{}|{}|{}|{}",
                call!("holds", QString::from("right")),
                get!("rightActionChoice", "value"),
                get!("rightActionSendNow", "visible"),
                get!("rightActionSendNow", "checked"),
                call!("previewIcon", QString::from("twoActionsPreview"), 1),
            );
            record!(label, held);
        }
        // On for this side only, once reader is told what a chat tapped by mistake gets and says
        // go ahead.
        record!("send-now-on", click!("rightActionSendNow"));
        record!(
            "send-now-asked",
            format!(
                "{}|{}",
                call!("stackLog"),
                call!("sendsNow", QString::from("right"))
            )
        );
        record!("send-now-warning", call!("warning"));
        record!("send-now-accept", call!("accept"));
        record!(
            "send-now-held",
            format!(
                "{}|{}|{}",
                call!("sendsNow", QString::from("right")),
                get!("rightActionSendNow", "checked"),
                call!("sendsNow", QString::from("left"))
            )
        );
        record!("left-switch", get!("leftActionSendNow", "visible"));
        // Off again at once, nothing asked.
        record!("send-now-off", click!("rightActionSendNow"));
        record!(
            "send-now-released",
            format!(
                "{}|{}|{}",
                call!("stackLog"),
                call!("sendsNow", QString::from("right")),
                get!("rightActionSendNow", "checked")
            )
        );
        // On again; camera action chosen anew (same one) starts off again.
        click!("rightActionSendNow");
        call!("accept");
        call!("stackLog");
        record!("send-now-again", call!("sendsNow", QString::from("right")));
        record!("pick-photo-again", click!("rightAction-photo"));
        record!(
            "photo-again-switch",
            format!(
                "{}|{}",
                call!("sendsNow", QString::from("right")),
                get!("rightActionSendNow", "checked")
            )
        );
        // So does any other action chosen.
        click!("rightActionSendNow");
        call!("accept");
        call!("stackLog");
        record!("pick-qr-again", click!("rightAction-qr"));
        record!(
            "qr-again-switch",
            format!(
                "{}|{}",
                get!("rightActionSendNow", "visible"),
                call!("sendsNow", QString::from("right"))
            )
        );

        // Chat asked for once choices page is gone and this one is back on screen;
        // nothing changes until picked.
        call!("setActive", QString::from("false"));
        record!(
            "pick-chat",
            call!("tapKind", QString::from("left"), QString::from("chat"))
        );
        record!("asked-at-once", call!("stackLog"));
        record!("unpicked-on", call!("onKind", QString::from("left")));
    });

    single_shot(Duration::from_millis(1500), move || unsafe {
        call!("setActive", QString::from("true"));
        record!("asked", call!("stackLog"));
        record!("unpicked", call!("holds", QString::from("left")));
        record!("unpicked-shown", get!("leftActionChoice", "value"));
        record!("picked", call!("pick", 2, QString::from("chat 2")));
        record!("left-chat", call!("holds", QString::from("left")));
        record!("picked-on", call!("onKind", QString::from("left")));
        // Back from picker: not asked twice.
        call!("setActive", QString::from("false"));
        call!("setActive", QString::from("true"));
        record!("asked-after-pick", call!("stackLog"));
        record!("icons-shown", get!("leftActionIcons", "visible"));
        record!("heart-lit", get!("leftIcon-heart", "highlighted"));
        record!(
            "two-left-heart",
            call!("previewIcon", QString::from("twoActionsPreview"), 0)
        );

        // Another icon, a later one.
        record!("pick-dog", click!("leftIcon-dog"));
        record!("left-dog", call!("holds", QString::from("left")));
        record!("dog-lit", get!("leftIcon-dog", "highlighted"));
        record!("heart-unlit", get!("leftIcon-heart", "highlighted"));
        record!(
            "two-left-dog",
            call!("previewIcon", QString::from("twoActionsPreview"), 0)
        );
    });

    // Chat by the name core has for it, in the choice; then it goes.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("chat-says", get!("leftActionChoice", "value"));
        record!("delete", call!("deleteChat", 1));
    });

    // Chosen again, "Chat" picks again: one that has gone.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "change-chat",
            call!("tapKind", QString::from("left"), QString::from("chat"))
        );
    });

    single_shot(Duration::from_millis(4500), move || unsafe {
        record!("asked-again", call!("stackLog"));
        record!("picked-gone", call!("pick", 1, QString::from("chat 1")));
    });

    single_shot(Duration::from_millis(5500), move || unsafe {
        record!("gone-says", get!("leftActionChoice", "value"));
        record!("left-gone", call!("holds", QString::from("left")));

        // Other profile's chat: picker opens on this one's, turned to the other's. Tapped while
        // choices page still going, waits for it.
        call!("setBusy", QString::from("true"));
        record!(
            "pick-elsewhere",
            call!("tapKind", QString::from("left"), QString::from("chat"))
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("asked-while-busy", call!("stackLog"));
        call!("setBusy", QString::from("false"));
        record!("asked-here", call!("stackLog"));
        record!(
            "picked-elsewhere",
            call!("pickIn", 2, 2, QString::from("chat 2"))
        );
        record!("left-elsewhere", call!("holds", QString::from("left")));
    });

    // By the name other profile has; picked again, picker opens on that profile.
    single_shot(Duration::from_secs(7), move || unsafe {
        record!("elsewhere-says", get!("leftActionChoice", "value"));
        record!(
            "pick-again",
            call!("tapKind", QString::from("left"), QString::from("chat"))
        );
    });

    single_shot(Duration::from_millis(7500), move || unsafe {
        record!("asked-there", call!("stackLog"));

        // Its profile deleted since: picker opens on one there is.
        record!("orphan", call!("orphan", QString::from("left"), 9));
        record!(
            "pick-orphan",
            call!("tapKind", QString::from("left"), QString::from("chat"))
        );
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!("asked-orphan", call!("stackLog"));

        // Back to room for one: right hidden and kept.
        record!("choose-one", click!("oneActionPreview"));
        record!("count-one", call!("count"));
        record!("right-hidden-again", get!("rightQuickAction", "visible"));
        record!("right-kept", call!("holds", QString::from("right")));
        record!("one-label-again", get!("leftActionChoice", "label"));
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

    assert_eq!(
        value("load-settings"),
        "ok",
        "the settings page did not load. {context}"
    );
    assert_eq!(
        value("under-advanced"),
        "quickActionsEntry",
        "the quick actions are not the first thing under Advanced. {context}"
    );
    assert_eq!(
        value("after-entry"),
        "webxdcSwitch",
        "the webxdc switch is not under Advanced with them. {context}"
    );
    // Calls had a switch here while experimental (#119).
    assert_eq!(
        value("after-webxdc"),
        "nothing",
        "something follows Advanced. {context}"
    );
    assert_eq!(
        value("old-heading"),
        "missing:Quick actions",
        "the quick actions still have a heading of their own. {context}"
    );
    assert_eq!(
        value("apps-heading"),
        "missing:Apps",
        "Apps is still a heading of its own. {context}"
    );
    assert_eq!(
        value("opened"),
        "push:QuickActionsPage.qml:1:undefined:undefined|",
        "the row does not open the quick actions' page for this profile. \
         {context}"
    );
    assert_eq!(
        value("load"),
        "ok",
        "the quick actions' page did not load. {context}"
    );
    assert_eq!(
        value("after-header"),
        "quickActionsExplained",
        "the quick actions are not explained right under the page's \
         header. {context}"
    );
    assert_eq!(
        value("after-words"),
        "quickActionCountChoices",
        "the pictures to choose between do not follow the explanation. \
         {context}"
    );
    assert_eq!(value("after-choices"), "leftQuickAction", "{context}");
    assert_eq!(value("after-left"), "rightQuickAction", "{context}");
    assert!(
        !value("words").is_empty(),
        "the explanation says nothing. {context}"
    );
    assert_eq!(
        value("one-chosen"),
        "true",
        "a fresh phone does not have room for one. {context}"
    );
    assert_eq!(value("two-unchosen"), "false", "{context}");
    assert_eq!(
        value("one-label"),
        "Action",
        "the one action is not called the action. {context}"
    );
    assert_eq!(
        value("right-hidden"),
        "false",
        "the right action shows with room for one. {context}"
    );
    assert_eq!(value("left-none-says"), "None", "{context}");
    assert_eq!(
        value("left-none-on"),
        "none",
        "the choice is not on what the setting holds. {context}"
    );
    assert_eq!(
        value("icons-hidden"),
        "false",
        "the icons show with no chat action. {context}"
    );
    assert_eq!(
        value("one-dot"),
        "",
        "the picture shows an icon for an action not chosen. {context}"
    );
    assert_eq!(value("left-search"), "search|0|0|", "{context}");
    assert_eq!(value("left-search-says"), "Search", "{context}");
    for label in ["one-search", "two-left-search"] {
        assert!(
            value(label).starts_with("search-"),
            "a picture does not show the search ({label}). {context}"
        );
    }
    assert_eq!(
        value("two-right-dot"),
        "",
        "the picture shows an icon for a right action not chosen. {context}"
    );
    assert_eq!(value("count-two"), "2", "{context}");
    assert_eq!(value("two-chosen"), "true", "{context}");
    assert_eq!(value("one-unchosen"), "false", "{context}");
    assert_eq!(value("left-label"), "Left", "{context}");
    assert_eq!(value("right-shown"), "true", "{context}");
    assert_eq!(value("right-qr-shown"), "My QR code", "{context}");
    assert!(
        value("two-right-qr").starts_with("qr-"),
        "the picture does not follow the right action. {context}"
    );
    assert_eq!(value("right-scan"), "scan|0|0|", "{context}");
    assert_eq!(value("right-profiles"), "profiles|0|0|", "{context}");
    assert_eq!(value("right-profiles-shown"), "Profiles", "{context}");
    assert_eq!(
        value("qr-switch"),
        "false",
        "the switch to send at once is under an action that takes nothing. \
         {context}"
    );
    for (kind, said, label) in CAMERA_KINDS {
        let expected = format!("ok|{kind}|0|0||{said}|true|false|{kind}-");
        assert!(
            value(label).starts_with(&expected),
            "the {kind} action is not written on the tap, said by name, \
             drawn with its icon, and shown with the switch to send at once \
             off: {:?} is not {expected:?}... {context}",
            value(label)
        );
    }
    assert_eq!(
        value("send-now-asked"),
        "push:ExperimentalFeatureDialog.qml:undefined:Send right away:undefined||false",
        "turning the switch on did not ask first, or did not wait for the \
         answer. {context}"
    );
    assert!(
        value("send-now-warning").contains("wrong chat"),
        "what is asked does not say a chat tapped by mistake gets it. \
         {context}"
    );
    assert_eq!(
        value("send-now-held"),
        "true|true|false",
        "turning the switch on is not written for this side, and this side \
         only, or the switch does not show it. {context}"
    );
    assert_eq!(
        value("left-switch"),
        "false",
        "the left action, which takes nothing, shows the switch. {context}"
    );
    assert_eq!(
        value("send-now-released"),
        "|false|false",
        "turning the switch off again asked something, or is not written. \
         {context}"
    );
    assert_eq!(
        (
            value("send-now-again").as_str(),
            value("photo-again-switch").as_str()
        ),
        ("true", "false|false"),
        "a camera action chosen anew keeps sending at once. {context}"
    );
    assert_eq!(
        value("qr-again-switch"),
        "false|false",
        "the switch shows under an action that takes nothing, or choosing \
         one kept sending at once. {context}"
    );
    assert!(
        value("two-right-profiles").starts_with("profiles-"),
        "the picture does not show the profiles' icon. {context}"
    );
    assert_eq!(value("right-scan-shown"), "Scan QR code", "{context}");
    assert_eq!(value("right-none-again"), "|0|0|", "{context}");
    assert_eq!(value("right-none-shown"), "None", "{context}");
    assert_eq!(
        value("asked-at-once"),
        "",
        "the chat picker was pushed while Silica's page of choices was \
         still over the page. {context}"
    );
    assert_eq!(
        value("asked"),
        "push:ChatPickerPage.qml:1:Choose a chat:true|",
        "choosing a chat does not ask which, from this profile. {context}"
    );
    assert_eq!(
        value("asked-after-pick"),
        "",
        "coming back from the chat picker asks for a chat again. {context}"
    );
    assert_eq!(
        (value("unpicked-on").as_str(), value("picked-on").as_str()),
        ("search", "chat"),
        "the choice does not stay on what the setting holds until a chat \
         is picked. {context}"
    );
    assert_eq!(
        value("unpicked"),
        "search|0|0|",
        "the action changed before a chat was picked. {context}"
    );
    assert_eq!(
        value("unpicked-shown"),
        "Search",
        "the choice shows a chat before one was picked. {context}"
    );
    assert_eq!(
        value("left-chat"),
        "chat|1|2|heart",
        "the picked chat is not the action's, with the first icon. {context}"
    );
    assert_eq!(value("icons-shown"), "true", "{context}");
    assert_eq!(value("heart-lit"), "true", "{context}");
    assert!(
        value("two-left-heart").starts_with("heart-"),
        "the picture does not show the chat's icon. {context}"
    );
    assert_eq!(value("left-dog"), "chat|1|2|dog", "{context}");
    assert_eq!(value("dog-lit"), "true", "{context}");
    assert_eq!(
        value("heart-unlit"),
        "false",
        "the icon picked before stays lit. {context}"
    );
    assert!(
        value("two-left-dog").starts_with("dog-"),
        "the picture does not follow the icon. {context}"
    );
    assert_eq!(
        value("chat-says"),
        "Chat: chat 2",
        "the choice does not say which chat it opens. {context}"
    );
    assert_eq!(
        value("asked-again"),
        "push:ChatPickerPage.qml:1:Choose a chat:true|",
        "choosing Chat again does not pick again. {context}"
    );
    assert_eq!(
        value("gone-says"),
        "Deleted chat",
        "an action whose chat has gone does not say so. {context}"
    );
    assert_eq!(
        value("left-gone"),
        "chat|1|1|dog",
        "picking another chat lost the icon. {context}"
    );
    assert_eq!(
        value("asked-while-busy"),
        "",
        "the chat picker was pushed while the page stack was moving. \
         {context}"
    );
    assert_eq!(
        value("asked-here"),
        "push:ChatPickerPage.qml:1:Choose a chat:true|",
        "the picker does not open on the chat's profile, with the others \
         offered. {context}"
    );
    assert_eq!(
        value("left-elsewhere"),
        "chat|2|2|dog",
        "a chat picked in another profile is not that profile's. {context}"
    );
    assert_eq!(
        value("elsewhere-says"),
        "Chat: chat 2",
        "another profile's chat is not said by its name. {context}"
    );
    assert_eq!(
        value("asked-there"),
        "push:ChatPickerPage.qml:2:Choose a chat:true|",
        "picked again, the picker does not open on the chat's profile. \
         {context}"
    );
    assert_eq!(
        value("asked-orphan"),
        "push:ChatPickerPage.qml:1:Choose a chat:true|",
        "an action whose profile was deleted opens the picker on that \
         profile, a list of nothing but \"account not found\". {context}"
    );
    assert_eq!(value("count-one"), "1", "{context}");
    assert_eq!(value("right-hidden-again"), "false", "{context}");
    assert_eq!(
        value("right-kept"),
        "qr|0|0|",
        "the right action was lost with room for one. {context}"
    );
    assert_eq!(value("one-label-again"), "Action", "{context}");
}
