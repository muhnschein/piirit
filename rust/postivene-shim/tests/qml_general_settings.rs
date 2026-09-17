//! The settings page, and the object every page reads the settings
//! through.
//!
//! Each control on the page writes one value of the `Settings` singleton,
//! and the choice it shows follows the value back -- so a change made
//! anywhere reaches the page, and a change made on the page reaches the
//! conversation. Both are loaded here against a stub `ConfigurationValue`
//! that holds a value and stores nothing.
//!
//! The deletion period is the one setting the page does not write on the
//! tap: it asks the core how much would go first, and the core here has
//! not been started, so what this pins is that nothing was written and
//! the page said why. The whole of that flow, dialog included, is
//! `qml_auto_delete_flow.rs`.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// The probe imports the app's components by absolute URL: it is loaded
/// from data, which has no directory of its own to resolve one against.
fn probe_qml() -> String {
    let components = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    format!(
        r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://{}'
    Item {{
        Loader {{ id: loader }}
        function load(url) {{
            loader.setSource(url, {{}})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        function findIn(node, name) {{
            if (!node) {{ return null }}
            if (node.objectName === name) {{ return node }}
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {{
                var hit = findIn(kids[i], name)
                if (hit) {{ return hit }}
            }}
            if (node.contentItem && node.contentItem !== node) {{
                return findIn(node.contentItem, name)
            }}
            // A ComboBox's items live in its menu, which is not among
            // its children.
            if (node.menu) {{
                var inMenu = findIn(node.menu, name)
                if (inMenu) {{ return inMenu }}
            }}
            return null
        }}
        function get(name, property) {{
            var item = findIn(loader.item, name)
            if (!item) {{ return 'missing:' + name }}
            return '' + item[property]
        }}
        function click(name) {{
            var item = findIn(loader.item, name)
            if (!item) {{ return 'missing:' + name }}
            item.clicked()
            return 'ok'
        }}
        // The app's side of the same keys.
        function appReads(name) {{ return '' + Settings[name] }}
        // What stands first under a section heading: the control after
        // the header that says `heading`.
        function firstUnder(heading) {{
            var header = findText(loader.item, heading)
            if (!header || !header.parent) {{ return 'missing:' + heading }}
            var kids = header.parent.children
            for (var i = 0; i + 1 < kids.length; i++) {{
                if (kids[i] === header) {{ return kids[i + 1].objectName }}
            }}
            return 'missing:' + heading
        }}
        function findText(node, text) {{
            if (!node) {{ return null }}
            if (node.text === text && node.objectName === '') {{ return node }}
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {{
                var hit = findText(kids[i], text)
                if (hit) {{ return hit }}
            }}
            return null
        }}
        function appWrites(name, value) {{ Settings[name] = value; return 'ok' }}
        function appKey(name) {{ return '' + Settings[name].key }}
        // Typing into the folder field: the field takes focus, as it
        // would under a finger, and the text changes under it.
        function typeFolder(text) {{
            var field = findIn(loader.item, 'folderField')
            if (!field) {{ return 'missing:folderField' }}
            field.forceActiveFocus()
            field.text = text
            return 'ok'
        }}
    }}
",
        components.display()
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_settings_page_writes_what_the_app_reads() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    // The page asks the core how much a deletion period would take; a
    // core never started answers that it is not there.
    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml()));

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
        // The keys, under the app's own path.
        record!(
            "app-enter-key",
            call!("appKey", QString::from("enterSendsConfig"))
        );
        record!(
            "app-markdown-key",
            call!("appKey", QString::from("markdownConfig"))
        );
        record!(
            "app-links-key",
            call!("appKey", QString::from("cleanLinksConfig"))
        );
        record!(
            "app-quality-key",
            call!("appKey", QString::from("mediaQualityConfig"))
        );
        record!(
            "app-download-key",
            call!("appKey", QString::from("downloadLimitConfig"))
        );
        record!(
            "app-deletion-key",
            call!("appKey", QString::from("deleteDeviceAfterConfig"))
        );
        record!(
            "app-notification-key",
            call!("appKey", QString::from("notificationDetailConfig"))
        );
        record!(
            "app-mentions-key",
            call!("appKey", QString::from("mentionNotificationsConfig"))
        );
        record!(
            "app-apps-key",
            call!("appKey", QString::from("webxdcEnabledConfig"))
        );
        record!(
            "app-folder-key",
            call!("appKey", QString::from("saveFolderConfig"))
        );
        record!(
            "app-notifications-key",
            call!("appKey", QString::from("notificationsEnabledConfig"))
        );
        record!(
            "load",
            call!("load", QString::from(common::page_url("SettingsPage.qml")))
        );
        // What a fresh phone shows. The return key is the first thing
        // under Messages, and puts in a line break until it is asked to
        // send.
        record!(
            "first-under-messages",
            call!("firstUnder", QString::from("Messages"))
        );
        record!(
            "enter-default",
            call!("appReads", QString::from("enterSends"))
        );
        record!("enter-switch", get!("enterSendsSwitch", "checked"));
        record!(
            "markdown-default",
            call!("appReads", QString::from("markdownMode"))
        );
        record!("markdown-switch", get!("markdownSwitch", "checked"));
        record!(
            "quality-default",
            call!("appReads", QString::from("mediaQuality"))
        );
        record!("quality-index", get!("qualityCombo", "currentIndex"));
        record!(
            "download-default",
            call!("appReads", QString::from("downloadLimit"))
        );
        record!("download-index", get!("downloadCombo", "currentIndex"));
        record!(
            "deletion-default",
            call!("appReads", QString::from("deleteDeviceAfter"))
        );
        record!("deletion-index", get!("deletionCombo", "currentIndex"));
        record!("deletion-label", get!("deletionCombo", "label"));
        record!(
            "links-default",
            call!("appReads", QString::from("cleanLinks"))
        );
        record!("links-switch", get!("cleanLinksSwitch", "checked"));
        record!(
            "notification-default",
            call!("appReads", QString::from("notificationDetail"))
        );
        record!(
            "notification-index",
            get!("notificationCombo", "currentIndex")
        );
        record!("notification-label", get!("notificationCombo", "label"));
        record!(
            "mentions-default",
            call!("appReads", QString::from("mentionNotifications"))
        );
        record!("mentions-switch", get!("mentionsSwitch", "checked"));
        record!(
            "apps-default",
            call!("appReads", QString::from("webxdcEnabled"))
        );
        record!("apps-switch", get!("webxdcSwitch", "checked"));
        // Each control writes its setting, and the choice shown follows it.
        record!(
            "flip-enter",
            call!("click", QString::from("enterSendsSwitch"))
        );
        record!("enter-on", call!("appReads", QString::from("enterSends")));
        record!("enter-switch-on", get!("enterSendsSwitch", "checked"));
        record!(
            "flip-enter-back",
            call!("click", QString::from("enterSendsSwitch"))
        );
        record!("enter-off", call!("appReads", QString::from("enterSends")));
        record!(
            "pick-markdown",
            call!("click", QString::from("markdownSwitch"))
        );
        record!(
            "markdown-picked",
            call!("appReads", QString::from("markdownMode"))
        );
        record!("markdown-shown", get!("markdownSwitch", "checked"));
        record!(
            "pick-worse",
            call!("click", QString::from("qualityOption1"))
        );
        record!(
            "quality-picked",
            call!("appReads", QString::from("mediaQuality"))
        );
        record!("quality-shown", get!("qualityCombo", "currentIndex"));
        record!(
            "pick-balanced",
            call!("click", QString::from("qualityOption0"))
        );
        record!(
            "balanced-picked",
            call!("appReads", QString::from("mediaQuality"))
        );
        record!("balanced-shown", get!("qualityCombo", "currentIndex"));
        record!(
            "pick-download",
            call!("click", QString::from("downloadOption32768"))
        );
        record!(
            "download-picked",
            call!("appReads", QString::from("downloadLimit"))
        );
        record!("download-shown", get!("downloadCombo", "currentIndex"));
        record!(
            "pick-always",
            call!("click", QString::from("downloadOption0"))
        );
        record!(
            "always-picked",
            call!("appReads", QString::from("downloadLimit"))
        );
        record!("always-shown", get!("downloadCombo", "currentIndex"));
        record!(
            "flip-links",
            call!("click", QString::from("cleanLinksSwitch"))
        );
        record!("links-on", call!("appReads", QString::from("cleanLinks")));
        record!("links-switch-on", get!("cleanLinksSwitch", "checked"));
        record!(
            "flip-back",
            call!("click", QString::from("cleanLinksSwitch"))
        );
        record!("links-off", call!("appReads", QString::from("cleanLinks")));
        record!(
            "pick-notification",
            call!("click", QString::from("notificationOption2"))
        );
        record!(
            "notification-picked",
            call!("appReads", QString::from("notificationDetail"))
        );
        record!(
            "notification-shown",
            get!("notificationCombo", "currentIndex")
        );
        // Mentions are on until the reader says otherwise, as the
        // reference clients have them.
        record!(
            "flip-mentions",
            call!("click", QString::from("mentionsSwitch"))
        );
        record!(
            "mentions-off",
            call!("appReads", QString::from("mentionNotifications"))
        );
        record!("mentions-switch-off", get!("mentionsSwitch", "checked"));
        record!(
            "flip-mentions-back",
            call!("click", QString::from("mentionsSwitch"))
        );
        record!(
            "mentions-on",
            call!("appReads", QString::from("mentionNotifications"))
        );
        // Apps are the one setting the rest of the app hides behind, so
        // both ways round it goes matter: off is what a phone that has
        // never been asked reads, and the switch says so.
        record!("flip-apps", call!("click", QString::from("webxdcSwitch")));
        record!("apps-on", call!("appReads", QString::from("webxdcEnabled")));
        record!("apps-switch-on", get!("webxdcSwitch", "checked"));
        record!(
            "flip-apps-back",
            call!("click", QString::from("webxdcSwitch"))
        );
        record!(
            "apps-off",
            call!("appReads", QString::from("webxdcEnabled"))
        );
        record!("apps-switch-off", get!("webxdcSwitch", "checked"));
        // A deletion period is not written on the tap: the core is asked
        // first, and this one is not there to answer.
        record!(
            "pick-deletion",
            call!("click", QString::from("deletionOption3600"))
        );
        record!(
            "deletion-unwritten",
            call!("appReads", QString::from("deleteDeviceAfter"))
        );
        record!("deletion-shown", get!("deletionCombo", "currentIndex"));
        // The other direction: a change made anywhere else reaches the
        // page's choice. A stored 2, which an older build wrote for "as
        // written", still reads as that.
        record!(
            "app-write",
            call!("appWrites", QString::from("markdownMode"), 2)
        );
        record!("page-follows", get!("markdownSwitch", "checked"));
        record!(
            "app-write-deletion",
            call!("appWrites", QString::from("deleteDeviceAfter"), 604_800)
        );
        record!("deletion-follows", get!("deletionCombo", "currentIndex"));

        // Whether anything is announced at all stands first under
        // Notifications, on by default, and off it greys out the two
        // settings under it rather than hiding them.
        record!(
            "first-under-notifications",
            call!("firstUnder", QString::from("Notifications"))
        );
        record!(
            "notifications-default",
            call!("appReads", QString::from("notificationsEnabled"))
        );
        record!(
            "notifications-switch",
            get!("notificationsSwitch", "checked")
        );
        record!("detail-usable", get!("notificationCombo", "enabled"));
        record!("mentions-usable", get!("mentionsSwitch", "enabled"));
        record!(
            "flip-notifications",
            call!("click", QString::from("notificationsSwitch"))
        );
        record!(
            "notifications-off",
            call!("appReads", QString::from("notificationsEnabled"))
        );
        record!(
            "notifications-switch-off",
            get!("notificationsSwitch", "checked")
        );
        record!("detail-greyed", get!("notificationCombo", "enabled"));
        record!("mentions-greyed", get!("mentionsSwitch", "enabled"));
        record!(
            "flip-notifications-back",
            call!("click", QString::from("notificationsSwitch"))
        );
        record!(
            "notifications-on",
            call!("appReads", QString::from("notificationsEnabled"))
        );
        record!("detail-usable-again", get!("notificationCombo", "enabled"));

        // Where a saved file goes: the app's own folder under Documents
        // until the reader types another, which becomes the setting once
        // the typing has paused. Emptied, it goes back to the default.
        record!(
            "folder-default",
            call!("appReads", QString::from("saveFolder"))
        );
        record!("folder-shown", get!("folderField", "text"));
        record!("folder-label", get!("folderField", "label"));
        record!(
            "type-folder",
            call!(
                "typeFolder",
                QString::from("/tmp/postivene-stub-standardpaths/Downloads/chat/")
            )
        );
        record!(
            "folder-unwritten",
            call!("appReads", QString::from("saveFolder"))
        );
    });
    // The core's refusal arrives a turn later; the typed folder lands
    // once the typing has paused.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("deletion-error", get!("errorBanner", "text"));
        record!(
            "folder-written",
            call!("appReads", QString::from("saveFolder"))
        );
        record!("folder-shown-written", get!("folderField", "text"));
        record!("type-nothing", call!("typeFolder", QString::from("  ")));
    });
    // Whole seconds: `single_shot` rounds, and the pause ends at 4.2.
    single_shot(Duration::from_secs(5), move || unsafe {
        record!(
            "folder-back-to-default",
            call!("appReads", QString::from("saveFolder"))
        );
        record!("folder-shown-default", get!("folderField", "text"));
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
        value("load"),
        "ok",
        "the settings page did not load. {context}"
    );
    for (label, expected) in [
        ("first-under-messages", "enterSendsSwitch"),
        ("app-enter-key", "/apps/harbour-postivene/enter_sends"),
        ("enter-default", "false"),
        ("enter-switch", "false"),
        ("flip-enter", "ok"),
        ("enter-on", "true"),
        ("enter-switch-on", "true"),
        ("flip-enter-back", "ok"),
        ("enter-off", "false"),
        ("markdown-default", "0"),
        ("markdown-switch", "true"),
        ("app-quality-key", "/apps/harbour-postivene/media_quality"),
        // Balanced, which is the core's own default and both reference
        // clients'.
        ("quality-default", "0"),
        ("quality-index", "0"),
        ("pick-worse", "ok"),
        ("quality-picked", "1"),
        ("quality-shown", "1"),
        ("pick-balanced", "ok"),
        ("balanced-picked", "0"),
        ("balanced-shown", "0"),
        ("download-default", "1048576"),
        ("download-index", "3"),
        ("deletion-default", "0"),
        ("deletion-index", "0"),
        ("deletion-label", "Delete messages from device"),
        ("links-default", "false"),
        ("links-switch", "false"),
        ("pick-markdown", "ok"),
        ("markdown-picked", "1"),
        ("markdown-shown", "false"),
        ("pick-download", "ok"),
        ("download-picked", "32768"),
        ("download-shown", "0"),
        ("pick-always", "ok"),
        ("always-picked", "0"),
        ("always-shown", "6"),
        ("flip-links", "ok"),
        ("links-on", "true"),
        ("links-switch-on", "true"),
        ("flip-back", "ok"),
        ("links-off", "false"),
        ("app-markdown-key", "/apps/harbour-postivene/markdown_mode"),
        ("app-links-key", "/apps/harbour-postivene/clean_links"),
        ("app-download-key", "/apps/harbour-postivene/download_limit"),
        (
            "app-deletion-key",
            "/apps/harbour-postivene/delete_device_after",
        ),
        (
            "app-notification-key",
            "/apps/harbour-postivene/notification_detail",
        ),
        (
            "app-mentions-key",
            "/apps/harbour-postivene/mention_notifications",
        ),
        ("app-apps-key", "/apps/harbour-postivene/webxdc_enabled"),
        ("app-folder-key", "/apps/harbour-postivene/save_folder"),
        (
            "app-notifications-key",
            "/apps/harbour-postivene/notifications_enabled",
        ),
        ("first-under-notifications", "notificationsSwitch"),
        ("notifications-default", "true"),
        ("notifications-switch", "true"),
        ("detail-usable", "true"),
        ("mentions-usable", "true"),
        ("flip-notifications", "ok"),
        ("notifications-off", "false"),
        ("notifications-switch-off", "false"),
        ("detail-greyed", "false"),
        ("mentions-greyed", "false"),
        ("flip-notifications-back", "ok"),
        ("notifications-on", "true"),
        ("detail-usable-again", "true"),
        (
            "folder-default",
            "/tmp/postivene-stub-standardpaths/Documents/Postivene",
        ),
        (
            "folder-shown",
            "/tmp/postivene-stub-standardpaths/Documents/Postivene",
        ),
        ("folder-label", "Save files to"),
        ("type-folder", "ok"),
        // Not on the keystroke.
        (
            "folder-unwritten",
            "/tmp/postivene-stub-standardpaths/Documents/Postivene",
        ),
        // Once the typing has paused, without the trailing slash.
        (
            "folder-written",
            "/tmp/postivene-stub-standardpaths/Downloads/chat",
        ),
        (
            "folder-shown-written",
            "/tmp/postivene-stub-standardpaths/Downloads/chat",
        ),
        ("type-nothing", "ok"),
        (
            "folder-back-to-default",
            "/tmp/postivene-stub-standardpaths/Documents/Postivene",
        ),
        (
            "folder-shown-default",
            "/tmp/postivene-stub-standardpaths/Documents/Postivene",
        ),
        ("apps-default", "false"),
        ("apps-switch", "false"),
        ("flip-apps", "ok"),
        ("apps-on", "true"),
        ("apps-switch-on", "true"),
        ("flip-apps-back", "ok"),
        ("apps-off", "false"),
        ("apps-switch-off", "false"),
        ("notification-default", "0"),
        ("notification-index", "0"),
        ("notification-label", "A new notification shows"),
        ("pick-notification", "ok"),
        ("notification-picked", "2"),
        ("notification-shown", "2"),
        ("mentions-default", "true"),
        ("mentions-switch", "true"),
        ("flip-mentions", "ok"),
        ("mentions-off", "false"),
        ("mentions-switch-off", "false"),
        ("flip-mentions-back", "ok"),
        ("mentions-on", "true"),
        ("pick-deletion", "ok"),
        ("deletion-unwritten", "0"),
        ("deletion-shown", "0"),
        ("app-write", "ok"),
        ("page-follows", "false"),
        ("app-write-deletion", "ok"),
        ("deletion-follows", "3"),
    ] {
        assert_eq!(value(label), expected, "{label} is wrong. {context}");
    }
    assert!(
        value("deletion-error").contains("not started"),
        "the page did not say why the period was not set, got {:?}. {context}",
        value("deletion-error")
    );
}
