//! Add-profile dialog behind profiles plus: create, backup, second device, each with its line;
//! one switch at a time, nothing preset, forward swipe only once picked. Picked page pushed on
//! top (back returns here); restore page told which half, also when page already built.

// Qt harness: see qml_new_group.rs.
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

// `pageStack` recorded, not performed; see qml_new_group.rs. `core` only says whether ready.
// `built` stands in for page Silica built from destination.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property QtObject core: QtObject { property string status: 'starting' }
        property QtObject pageStack: QtObject {
            /// `push:Foo.qml{...}|...`
            property string log: ''
            function note(verb, page, properties) {
                var name = ('' + page).split('/').pop()
                log = log + verb + ':' + name + JSON.stringify(properties) + '|'
            }
            function push(page, properties) { note('push', page, properties) }
            function replace(page, properties) { note('replace', page, properties) }
        }
        QtObject { id: built; property string from: 'unset' }
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, {})
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
        /// Switch's words and line, `|`-joined.
        function words(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return item.text + '|' + item.description
        }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        /// All three switches, as `create/file/device`.
        function switches() {
            return ['createSwitch', 'fileSwitch', 'deviceSwitch'].map(function(name) {
                var item = findIn(loader.item, name)
                return item ? '' + item.checked : 'missing'
            }).join('/')
        }
        function createEnabled() { return '' + findIn(loader.item, 'createSwitch').enabled }
        function canAccept() { return '' + loader.item.canAccept }
        function coreReady() { core.status = 'ready'; return 'ok' }
        /// Silica built restore page on given half; reader may then change mind.
        function build(from) {
            built.from = from
            loader.item.acceptDestinationInstance = built
            return 'ok'
        }
        function builtFrom() { return built.from }
        /// Forward swipe (Silica allows it only while `canAccept`); what it pushed.
        function swipeForward() {
            var before = pageStack.log.length
            loader.item.accept()
            return pageStack.log.substring(before)
        }
        function header(property) { return '' + findIn(loader.item, 'header')[property] }
        function quickActions() { return '' + loader.item.pausesQuickActions }
        function navigation() { return pageStack.log }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_add_profile_dialog_offers_create_backup_and_second_device() {
    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    piirit_shim::register_qml_types();

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
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

    let steps: common::Steps = common::Steps::default();
    let taken = steps.clone();
    // Loads, builds switches, taps, accepts: all synchronous once loop runs.
    single_shot(Duration::from_secs(1), move || unsafe {
        let record = |label: &str, value: String| common::record(&taken, label, value.into());
        record(
            "load",
            call!(
                "load",
                QString::from(common::page_url("AddProfileWaysDialog.qml"))
            ),
        );
        record("title", call!("header", QString::from("title")));
        record("accept-text", call!("header", QString::from("acceptText")));
        record("quick-actions", call!("quickActions"));
        for name in ["createSwitch", "fileSwitch", "deviceSwitch"] {
            record(name, call!("words", QString::from(name)));
        }
        record("fresh", call!("switches"));
        record("fresh-accept", call!("canAccept"));
        record("fresh-swipe", call!("swipeForward"));

        // Core not up yet: create greyed, and no way on even if picked.
        record("early-enabled", call!("createEnabled"));
        call!("tap", QString::from("createSwitch"));
        record("early-accept", call!("canAccept"));
        record("early-swipe", call!("swipeForward"));
        call!("coreReady");
        record("ready-enabled", call!("createEnabled"));
        record("create", call!("switches"));
        record("create-accept", call!("canAccept"));
        record("create-swipe", call!("swipeForward"));
        // Tap again: nothing picked.
        call!("tap", QString::from("createSwitch"));
        record("create-off", call!("switches"));
        record("create-off-accept", call!("canAccept"));

        // Page Silica built before backup picked: restore page's own default half.
        call!("build", QString::from("device"));
        call!("tap", QString::from("fileSwitch"));
        record("file", call!("switches"));
        record("file-swipe", call!("swipeForward"));
        record("file-built", call!("builtFrom"));
        // Straight from backup to second device; Silica kept page it built for backup.
        call!("build", QString::from("file"));
        call!("tap", QString::from("deviceSwitch"));
        record("device", call!("switches"));
        record("device-swipe", call!("swipeForward"));
        record("device-built", call!("builtFrom"));
        record("navigation", call!("navigation"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow().clone();
    let context = format!("steps: {steps:?}");
    let value = |label: &str| common::value_of(&steps, label).to_string();
    assert_eq!(value("load"), "ok", "the dialog did not load. {context}");
    // Plus row said "Add profile": title would repeat it.
    assert_eq!(
        (value("title").as_str(), value("accept-text").as_str()),
        ("", "Next"),
        "the dialog has a title, or lacks Next to go on. {context}"
    );
    assert_eq!(
        value("quick-actions"),
        "true",
        "quick actions are not paused while a profile is being added. {context}"
    );
    assert_eq!(
        value("createSwitch"),
        "Create a profile|You start with a new address and no chats.",
        "the create choice lacks its name or its line. {context}"
    );
    assert_eq!(
        value("fileSwitch"),
        "Restore from a backup|You import a profile from a backup file.",
        "the backup choice lacks its name or its line. {context}"
    );
    assert_eq!(
        value("deviceSwitch"),
        "Add as second device|You use the same profile on this phone and another device.",
        "the second-device choice lacks its name or its line. {context}"
    );
    assert_eq!(
        (
            value("fresh").as_str(),
            value("fresh-accept").as_str(),
            value("fresh-swipe").as_str()
        ),
        ("false/false/false", "false", ""),
        "something is picked before the reader picked, or the dialog goes on with nothing \
         picked. {context}"
    );
    assert_eq!(
        (
            value("early-enabled").as_str(),
            value("early-accept").as_str(),
            value("early-swipe").as_str()
        ),
        ("false", "false", ""),
        "a profile can be created before the core is ready. {context}"
    );
    assert_eq!(
        (
            value("ready-enabled").as_str(),
            value("create").as_str(),
            value("create-accept").as_str()
        ),
        ("true", "true/false/false", "true"),
        "with the core ready, create is not picked alone, ready to go on. {context}"
    );
    assert_eq!(
        value("create-swipe"),
        "push:AddProfileDialog.qml{}|",
        "create does not push the name and relay dialog. {context}"
    );
    assert_eq!(
        (
            value("create-off").as_str(),
            value("create-off-accept").as_str()
        ),
        ("false/false/false", "false"),
        "a second tap does not take the pick back. {context}"
    );
    assert_eq!(
        value("file"),
        "false/true/false",
        "tapping backup did not pick backup alone. {context}"
    );
    assert_eq!(
        value("file-swipe"),
        r#"push:RestoreProfilePage.qml{"from":"file"}|"#,
        "backup does not push the restore page on its file half. {context}"
    );
    assert_eq!(
        value("file-built"),
        "file",
        "the restore page built before backup was picked does not open on its file half. \
         {context}"
    );
    assert_eq!(
        value("device"),
        "false/false/true",
        "tapping second device did not turn backup off. {context}"
    );
    assert_eq!(
        value("device-swipe"),
        r#"push:RestoreProfilePage.qml{"from":"device"}|"#,
        "second device does not push the restore page on its device half. {context}"
    );
    // Silica builds page once per destination: properties alone would leave it on file half.
    assert_eq!(
        value("device-built"),
        "device",
        "the restore page built for backup opens on its file half after the reader picked \
         second device. {context}"
    );
    // Pushed, not replaced: back from picked page comes to this dialog.
    assert!(
        !value("navigation").contains("replace:"),
        "a choice took this dialog's place, so back would skip it. {context}"
    );
}
