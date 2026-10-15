//! Pages a reader with a profile already meets.
//!
//! Two: where that profile is (`ExistingProfilePage.qml`), and the page bringing it
//! over (`RestoreProfilePage.qml`), serving both answers. Checked: which way each
//! button leads; file half offers file, device half does not; transfer takes reader
//! to the profile brought over; a code the shim refuses is put into words, not shown
//! as a protocol complaint. Transfer itself: `restore.rs`.
//!
//! Device half must give camera the page down to its foot and say what it looks for.
//! Code is read from pixels it lands in; a viewfinder sharing the page with a
//! paragraph gave too few to read reliably.

// Qt harness: `unsafe` `env::set_var` before Qt starts (`unused_unsafe`: unsafe only from
// edition 2024), `borrow_as_ptr` for engine pointer, `single_shot` with whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! params must match dispatcher's by-value params; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Records navigation, not performed. Property that matters: which way take-over page opened.
#[allow(non_snake_case)]
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:RestoreProfilePage.qml(device)|replaceAbove:ChatListPage.qml|`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),

    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    replaceAbove:
        qt_method!(fn(&mut self, target: QVariant, page: QString, properties: QVariantMap)),
    pop: qt_method!(fn(&mut self)),
}

#[allow(non_snake_case)]
impl PageStackProbe {
    fn record(&mut self, entry: &str) {
        let current = self.log.to_string();
        self.log = format!("{current}{entry}|").into();
        self.log_changed();
    }

    fn name(page: &QString) -> String {
        let page = page.to_string();
        page.rsplit('/').next().unwrap_or(&page).to_string()
    }

    fn push(&mut self, page: QString, properties: QVariantMap) {
        // `QVariantMap` in qttypes 0.2: queried, not iterated.
        let from =
            QString::from_qvariant(properties.value(QString::from("from"), QVariant::default()))
                .map(|value| value.to_string())
                .unwrap_or_default();
        let name = Self::name(&page);
        if from.is_empty() {
            self.record(&format!("push:{name}"));
        } else {
            self.record(&format!("push:{name}({from})"));
        }
    }

    fn replaceAbove(&mut self, _target: QVariant, page: QString, _properties: QVariantMap) {
        self.record(&format!("replaceAbove:{}", Self::name(&page)));
    }

    fn pop(&mut self) {
        self.record("pop");
    }
}

/// Owns a `Loader`; works loaded page by `objectName`.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }

        function loadWith(url, json) {
            loader.setSource('', {})
            loader.setSource(url, JSON.parse(json))
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // What Silica does when a page becomes the one on screen.
        function activate() { loader.item.status = 2; return 'ok' }
        // The app sent to the background, or the screen turned off, and
        // brought back: Silica leaves the page's status as it was.
        function background(away) { loader.item.appActive = away !== 'true'; return 'ok' }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function pageProperty(property) {
            return loader.item ? '' + loader.item[property] : 'no-page'
        }
        // Where an item ends, measured from the top of the page.
        function bottomOf(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + (item.y + item.height)
        }
        // A property of the scanner the page loaded rather than of the
        // page: how the page has set the view up.
        function viewProperty(property) {
            var host = findIn(loader.item, 'scanLoader')
            if (!host) { return 'missing:scanLoader' }
            if (!host.item) { return 'no-view:' + host.status }
            return '' + host.item[property]
        }
        // The page turned on its side, and what the view was told: the
        // camera's picture is turned back by the page's turn.
        function turnPage(orientation) {
            var host = findIn(loader.item, 'scanLoader')
            if (!host || !host.item) { return 'no-view' }
            var before = loader.item.orientation
            loader.item.orientation = parseInt(orientation)
            var told = '' + host.item.pageOrientation
            loader.item.orientation = before
            return told
        }
        // What the file browser hands the page.
        function begin(text) {
            if (!loader.item) { return 'no-page' }
            loader.item.begin(text)
            return 'ok'
        }
        // What the camera does: the view reports the code it read and
        // stops itself, and the page hears it through the connection
        // the page made. Not the same as calling begin() -- stopping
        // itself is the state a retry has to undo.
        function scan(text) {
            var host = findIn(loader.item, 'scanLoader')
            if (!host || !host.item) { return 'no-view' }
            host.item.found(text)
            return 'ok'
        }
    }
";

// Engine, QObject boxes, steps: one scope, must outlive `exec()`. Asserts in helper below.
#[allow(clippy::too_many_lines)]
#[test]
fn a_profile_that_exists_already_is_asked_after_and_brought_over() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-restore-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // SAFETY: single-threaded; set before Qt and server start.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Where view writes grabbed frames.
        std::env::set_var("XDG_CACHE_HOME", &temp);
    }

    // Scanner is a `Piirit` type: without this view fails to load and
    // device half shows camera error.
    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(PageStackProbe::default());

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    // SAFETY: callbacks fire only while `exec()` runs on this thread; `engine` outlives it.
    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = unsafe {
                (*engine_ptr).invoke_method(
                    $name.into(),
                    &[$(QVariant::from(QString::from($arg))),*],
                )
            };
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }

    let steps: common::Steps = Rc::new(RefCell::new(Vec::new()));

    // 1s: question, and both answers.
    let s = steps.clone();
    single_shot(Duration::from_secs(1), move || {
        common::record(
            &s,
            "ask-load",
            call!(
                "loadWith",
                common::page_url("ExistingProfilePage.qml"),
                "{}"
            ),
        );
        common::record(&s, "ask-device", call!("click", "secondDeviceTile"));
        common::record(&s, "ask-file", call!("click", "backupFileTile"));
        // Same question asked of reader with a profile, from plus under profiles list: three
        // ways, not two (making one is the third).
        common::record(
            &s,
            "add-load",
            call!("loadWith", common::page_url("AddProfilePage.qml"), "{}"),
        );
        common::record(&s, "add-stacked", call!("get", "addWays", "stacked"));
        common::record(&s, "add-header", call!("get", "header", "height"));
        common::record(&s, "add-ways-y", call!("get", "addWays", "y"));
        common::record(&s, "add-ways-height", call!("get", "addWays", "height"));
        common::record(&s, "add-page-height", call!("pageProperty", "height"));
        common::record(&s, "add-create", call!("click", "createProfileTile"));
        common::record(&s, "add-file", call!("click", "backupFileTile"));
        common::record(&s, "add-device", call!("click", "secondDeviceTile"));
    });

    // 2s: file half. Browser offered, camera not; choosing opens browser.
    let s = steps.clone();
    single_shot(Duration::from_secs(2), move || {
        common::record(
            &s,
            "file-load",
            call!(
                "loadWith",
                common::page_url("RestoreProfilePage.qml"),
                r#"{"from":"file","status":2}"#
            ),
        );
        common::record(
            &s,
            "file-button",
            call!("get", "chooseFileButton", "visible"),
        );
        common::record(&s, "file-scanner", call!("get", "scanArea", "visible"));
        common::record(&s, "file-choose", call!("click", "chooseFileButton"));
        // Browser reports back: import starts.
        common::record(&s, "file-begin", call!("begin", "/tmp/holiday-backup.tar"));
        common::record(&s, "file-busy", call!("pageProperty", "busy"));
    });

    // 4s: core answered; page on the profile brought over.
    let s = steps.clone();
    single_shot(Duration::from_secs(4), move || {
        common::record(&s, "file-done", call!("pageProperty", "busy"));
    });

    // 5s: device half, handed code from newer Delta Chat than this core reads.
    let s = steps.clone();
    single_shot(Duration::from_secs(5), move || {
        common::record(
            &s,
            "device-load",
            call!(
                "loadWith",
                common::page_url("RestoreProfilePage.qml"),
                r#"{"from":"device","status":2,"appActive":true}"#
            ),
        );
        common::record(
            &s,
            "device-button",
            call!("get", "chooseFileButton", "visible"),
        );
    });

    // 6s: what camera was given and view was told, read before code handed over:
    // transfer takes viewfinder down.
    let s = steps.clone();
    single_shot(Duration::from_secs(6), move || {
        common::record(&s, "device-foot", call!("bottomOf", "scanArea"));
        common::record(&s, "device-page", call!("pageProperty", "height"));
        common::record(&s, "device-link", call!("viewProperty", "offerLink"));
        common::record(&s, "device-typed", call!("get", "typeLinkButton", "text"));
        common::record(
            &s,
            "device-field",
            call!("get", "linkField", "placeholderText"),
        );
        common::record(&s, "device-prefixes", call!("viewProperty", "linkPrefixes"));
        common::record(&s, "device-turned", call!("turnPage", "2"));
        common::record(
            &s,
            "device-begin",
            call!("scan", "DCBACKUP2:toonew.example"),
        );
    });

    let s = steps.clone();
    single_shot(Duration::from_secs(8), move || {
        common::record(&s, "device-busy", call!("pageProperty", "busy"));
        common::record(&s, "device-said", call!("pageProperty", "errorMessage"));
        // Failure is the page, not a strip along a viewfinder that carried on without it.
        common::record(&s, "failed-block", call!("get", "failureBlock", "visible"));
        common::record(&s, "failed-scanner", call!("get", "scanArea", "visible"));
        common::record(&s, "failed-retry", call!("click", "retryButton"));
    });

    // 9s: after retry camera back, reading again, not holding code it stopped on.
    let s = steps.clone();
    single_shot(Duration::from_secs(9), move || {
        common::record(&s, "retried-scanner", call!("get", "scanArea", "visible"));
        common::record(&s, "retried-holding", call!("viewProperty", "done"));
        common::record(&s, "retried-camera", call!("get", "camera", "running"));
        // In background camera stops, frames stop grabbing; comes back with app.
        common::record(&s, "away", call!("background", "true"));
        common::record(&s, "away-camera", call!("get", "camera", "running"));
        common::record(&s, "away-grabbing", call!("get", "grabber", "running"));
        common::record(&s, "back", call!("background", "false"));
        common::record(&s, "back-camera", call!("get", "camera", "running"));
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_pages(&steps.borrow(), &navigation);
}

/// Question leads both ways; file half offers browser not camera, chosen file imported;
/// arriving profile is what app lands on; refused code shown as sentence.
fn assert_pages(steps: &[(String, String)], navigation: &str) {
    let context = format!("steps: {steps:?}\nnavigation: {navigation}");

    assert_eq!(
        common::value_of(steps, "ask-load"),
        "ok",
        "the question did not load. {context}"
    );
    assert!(
        navigation.contains("push:RestoreProfilePage.qml(device)"),
        "the second-device answer did not open the take-over page on the \
         device half. {context}"
    );
    assert!(
        navigation.contains("push:RestoreProfilePage.qml(file)"),
        "the backup answer did not open the take-over page on the file \
         half. {context}"
    );

    assert_eq!(
        common::value_of(steps, "file-load"),
        "ok",
        "the take-over page did not load. {context}"
    );
    assert_eq!(
        common::value_of(steps, "file-button"),
        "true",
        "the file half does not offer the file browser. {context}"
    );
    assert_eq!(
        common::value_of(steps, "file-scanner"),
        "false",
        "the file half put the camera up anyway. {context}"
    );
    for label in ["add-load", "add-create", "add-file", "add-device"] {
        assert_eq!(
            common::value_of(steps, label),
            "ok",
            "the page behind the profiles plus does not offer all three \
             ways in: {label}. {context}"
        );
    }
    // Three, one under another: side by side each gets a third of screen, not room for a
    // line plus second line.
    assert_eq!(
        common::value_of(steps, "add-stacked"),
        "true",
        "the three ways in are crowded into one row. {context}"
    );
    // Stack starts under header, not mid-page: lists read from top, as every phone list does.
    let measure =
        |label: &str| -> f64 { common::value_of(steps, label).parse().unwrap_or_default() };
    let above = measure("add-ways-y") - measure("add-header");
    let below = measure("add-page-height") - measure("add-ways-y") - measure("add-ways-height");
    assert!(
        above > 0.0 && above * 4.0 < below,
        "the three ways do not start under the header: {above} above, \
         {below} below. {context}"
    );
    assert!(
        navigation.contains("push:AddProfileDialog.qml"),
        "nothing on the add-profile page makes a new profile. {context}"
    );
    assert!(
        navigation.contains("push:BackupFilePage.qml"),
        "choosing a backup did not open the file browser. {context}"
    );
    assert_eq!(
        common::value_of(steps, "file-busy"),
        "true",
        "the import did not start on the file that was chosen. {context}"
    );
    assert_eq!(
        common::value_of(steps, "file-done"),
        "false",
        "the page is still transferring after the core has answered. \
         {context}"
    );
    assert!(
        navigation.contains("replaceAbove:ChatListPage.qml"),
        "the profile that arrived is not what the app landed on. {context}"
    );

    assert_eq!(
        common::value_of(steps, "device-button"),
        "false",
        "the device half offers the file browser as well. {context}"
    );

    assert_device_half(steps, &context);
    assert_scanner_sleeps(steps, &context);
    assert_scanner_turns(steps, &context);
}

/// In background camera stops, frames stop grabbing; comes back with app.
fn assert_scanner_sleeps(steps: &[(String, String)], context: &str) {
    assert_eq!(common::value_of(steps, "away"), "ok", "{context}");
    assert_eq!(
        [
            common::value_of(steps, "away-camera"),
            common::value_of(steps, "away-grabbing"),
        ],
        ["false", "false"],
        "the camera, or the grabbing of its frames, carried on with the \
         app in the background. {context}"
    );
    assert_eq!(
        common::value_of(steps, "back-camera"),
        "true",
        "the camera did not come back with the app. {context}"
    );
}

/// Device half reads codes: camera has page down to foot, view worded for what it looks
/// for, same string typeable if camera will not read, refused code comes back as sentence.
fn assert_device_half(steps: &[(String, String)], context: &str) {
    // Camera runs to foot of page, as on QR page. Measured at viewfinder's end, not height:
    // header and instructions sit above; all left after them is camera's.
    let foot: f64 = common::value_of(steps, "device-foot")
        .parse()
        .unwrap_or(0.0);
    let page: f64 = common::value_of(steps, "device-page")
        .parse()
        .unwrap_or(0.0);
    assert!(
        page > 0.0 && page - foot <= 64.0,
        "the camera stops {} short of the foot of a {page} page, so it \
         is a strip with the page scrolling under it -- which is the \
         view a code cannot be read out of. {context}",
        page - foot
    );

    // Camera that will not read is dead end without this; other device shows same string as
    // text beside its code.
    assert_eq!(
        common::value_of(steps, "device-link"),
        "true",
        "the take-over scanner gives a reader no way past a camera that \
         will not read the code. {context}"
    );
    let typed = common::value_of(steps, "device-typed");
    assert!(
        !typed.to_lowercase().contains("invite"),
        "the typed half is still worded for an invite: {typed:?}. \
         {context}"
    );
    assert_eq!(
        common::value_of(steps, "device-field"),
        "DCBACKUP5:...",
        "the field does not show what it is waiting for. The digit is \
         the core's transfer version, and the pinned core -- the one \
         this app's own second-device page shows a code from -- speaks \
         5. {context}"
    );
    // Decides whether clipboard is worth pasting. Naming a version here is how `DCBACKUP5:`
    // (this app's own provider) got refused by the paste it is meant for.
    assert_eq!(
        common::value_of(steps, "device-prefixes"),
        "dcbackup",
        "the paste check names a transfer version, so a code from a core \
         speaking another one is not offered to the field. {context}"
    );
    assert_eq!(
        common::value_of(steps, "failed-block"),
        "true",
        "a failed transfer is not said anywhere the reader will see it. \
         {context}"
    );
    assert_eq!(
        common::value_of(steps, "failed-scanner"),
        "false",
        "the camera came straight back over the failure, which reads as \
         the app having carried on. {context}"
    );
    assert_eq!(
        common::value_of(steps, "failed-retry"),
        "ok",
        "there is no way to have another go. {context}"
    );
    assert_eq!(
        common::value_of(steps, "retried-scanner"),
        "true",
        "the retry did not bring the camera back. {context}"
    );
    assert_eq!(
        common::value_of(steps, "retried-holding"),
        "false",
        "the view is still holding the code it stopped itself on, so it \
         will never read another. {context}"
    );
    assert_eq!(
        common::value_of(steps, "retried-camera"),
        "true",
        "the retry brought back a viewfinder with the camera off behind \
         it. {context}"
    );

    assert_eq!(
        common::value_of(steps, "device-begin"),
        "ok",
        "the code from the camera was not acted on. {context}"
    );
    assert_eq!(
        common::value_of(steps, "device-busy"),
        "false",
        "the page waits on a code the shim already refused. {context}"
    );
    let said = common::value_of(steps, "device-said");
    assert!(
        said.contains("newer"),
        "the refusal was not put into words for the reader: {said:?}. \
         {context}"
    );
}

/// Page turns with phone; scanner it loaded is told, so camera picture turns back, not with page.
fn assert_scanner_turns(steps: &[(String, String)], context: &str) {
    assert_eq!(
        common::value_of(steps, "device-turned"),
        "2",
        "the page turned with the phone and the scanner was not told, so its picture \
         turns with the page. {context}"
    );
}
