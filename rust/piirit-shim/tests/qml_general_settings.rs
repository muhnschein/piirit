//! Settings page and `Settings` singleton every page reads through. Each control writes one value;
//! shown choice follows it back, so a change anywhere reaches page, and one on page reaches
//! conversation. Both loaded vs stub `ConfigurationValue` that holds a value, stores nothing.
//!
//! Deletion period is the one setting not written on tap: core asked how much would go first, and
//! core here isn't started, so pinned: nothing written, page said why. Full flow, dialog included:
//! `qml_auto_delete_flow.rs`.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use std::path::PathBuf;

mod common;

/// Probe imports components by absolute URL: data has no dir.
fn probe_qml() -> String {
    let components = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    format!(
        r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://{}'
    Item {{
        Loader {{ id: loader }}
        // Silica's page stack, as far as the page uses it here: push
        // records what was pushed and hands back a dialog for the page
        // to connect to, which the test accepts or leaves.
        QtObject {{
            id: stack
            property string pushed: ''
            property bool busy: false
            property QtObject dialog: QtObject {{
                signal accepted()
            }}
            function push(url, props) {{
                pushed = ('' + url).split('/').pop() + ':title=' + props.title
                return dialog
            }}
        }}
        function stackObject() {{ return stack }}
        function pushed() {{ var was = stack.pushed; stack.pushed = ''; return was }}
        function acceptDialog() {{ stack.dialog.accepted(); return 'ok' }}
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
        // What stands last in the section before a heading: the control
        // before the header that says `heading`.
        function lastBefore(heading) {{
            var header = findText(loader.item, heading)
            if (!header || !header.parent) {{ return 'missing:' + heading }}
            var kids = header.parent.children
            for (var i = 1; i < kids.length; i++) {{
                if (kids[i] === header) {{ return kids[i - 1].objectName }}
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
    }}
",
        components.display()
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_settings_page_writes_what_the_app_reads() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    // Quick actions look chat up via shim types.
    piirit_shim::register_qml_types();

    // Deletion-size question: unstarted core answers not there.
    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml()));
    // Named for page before load, as Silica does.
    let stack = engine.invoke_method("stackObject".into(), &[]);
    engine.set_property("pageStack".into(), stack);

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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        // Keys under app's own path.
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
            "app-notifications-key",
            call!("appKey", QString::from("notificationsEnabledConfig"))
        );
        record!(
            "load",
            call!("load", QString::from(common::page_url("SettingsPage.qml")))
        );
        // Fresh phone: what leaves the phone first under Messages, how a message is drawn last.
        record!(
            "first-under-messages",
            call!("firstUnder", QString::from("Messages"))
        );
        record!(
            "last-under-messages",
            call!("lastBefore", QString::from("Notifications"))
        );
        // Return key: line break only; no setting for it, on page or behind it.
        record!("enter-switch", get!("enterSendsSwitch", "checked"));
        record!(
            "enter-setting",
            call!("appReads", QString::from("enterSends"))
        );
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
            "mentions-description",
            get!("mentionsSwitch", "description")
        );
        record!(
            "apps-default",
            call!("appReads", QString::from("webxdcEnabled"))
        );
        record!("apps-switch", get!("webxdcSwitch", "checked"));
        // Each control writes its setting; shown choice follows.
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
        // Mentions on until reader says otherwise, as reference clients.
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
        // Apps: the one setting rest of app hides behind, so both directions matter. Off is what a
        // never-asked phone reads, and switch says so. Experimental on asks first; off doesn't.
        record!("flip-apps", call!("click", QString::from("webxdcSwitch")));
        record!("apps-asked", call!("pushed"));
        record!(
            "apps-on-tap",
            call!("appReads", QString::from("webxdcEnabled"))
        );
        record!("accept-apps", call!("acceptDialog"));
        record!("apps-on", call!("appReads", QString::from("webxdcEnabled")));
        record!("apps-switch-on", get!("webxdcSwitch", "checked"));
        record!(
            "flip-apps-back",
            call!("click", QString::from("webxdcSwitch"))
        );
        record!("apps-off-asked", call!("pushed"));
        record!(
            "apps-off",
            call!("appReads", QString::from("webxdcEnabled"))
        );
        record!("apps-switch-off", get!("webxdcSwitch", "checked"));
        // Calls not experimental (#119): no switch, nothing asked; ringing is own setting, usable
        // on a never-asked phone.
        record!("calls-switch", get!("callsSwitch", "visible"));
        record!("calls-ring-usable", get!("callsRingSwitch", "enabled"));
        record!("calls-ring-default", get!("callsRingSwitch", "checked"));
        // Deletion period not written on tap: core asked first; absent here.
        record!(
            "pick-deletion",
            call!("click", QString::from("deletionOption3600"))
        );
        record!(
            "deletion-unwritten",
            call!("appReads", QString::from("deleteDeviceAfter"))
        );
        record!("deletion-shown", get!("deletionCombo", "currentIndex"));
        // Change elsewhere reaches page. Stored 2 (older build's "as written") reads as that.
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

        // Announce-at-all first under Notifications, default on; off greys two settings under it,
        // not hides them.
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
    });
    // Core refusal arrives a turn later.
    plan.after(1, move || unsafe {
        record!("deletion-error", get!("errorBanner", "text"));
        (*engine_ptr).quit();
    });

    plan.run();
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
        ("first-under-messages", "qualityCombo"),
        ("last-under-messages", "markdownSwitch"),
        ("enter-switch", "missing:enterSendsSwitch"),
        ("enter-setting", "undefined"),
        ("markdown-default", "0"),
        ("markdown-switch", "true"),
        ("app-quality-key", "/apps/harbour-piirit/media_quality"),
        // Balanced: core default, both reference clients'.
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
        ("app-markdown-key", "/apps/harbour-piirit/markdown_mode"),
        ("app-links-key", "/apps/harbour-piirit/clean_links"),
        ("app-download-key", "/apps/harbour-piirit/download_limit"),
        (
            "app-deletion-key",
            "/apps/harbour-piirit/delete_device_after",
        ),
        (
            "app-notification-key",
            "/apps/harbour-piirit/notification_detail",
        ),
        (
            "app-mentions-key",
            "/apps/harbour-piirit/mention_notifications",
        ),
        ("app-apps-key", "/apps/harbour-piirit/webxdc_enabled"),
        ("apps-default", "false"),
        ("apps-switch", "false"),
        ("flip-apps", "ok"),
        (
            "apps-asked",
            "ExperimentalFeatureDialog.qml:title=Enable webxdc apps (experimental)",
        ),
        ("apps-on-tap", "false"),
        ("accept-apps", "ok"),
        ("apps-on", "true"),
        ("apps-switch-on", "true"),
        ("flip-apps-back", "ok"),
        ("apps-off-asked", ""),
        ("apps-off", "false"),
        ("apps-switch-off", "false"),
        ("calls-switch", "missing:callsSwitch"),
        ("calls-ring-usable", "true"),
        ("calls-ring-default", "true"),
        (
            "app-notifications-key",
            "/apps/harbour-piirit/notifications_enabled",
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
    ] {
        assert_eq!(value(label), expected, "{label} is wrong. {context}");
    }
    // Only quoted replies pass a mute (`is_mention`); reactions never notify. Words promised both.
    let mentions = value("mentions-description");
    assert!(
        mentions.contains("replies") && !mentions.contains("reaction"),
        "the Mentions switch promised what muted groups do not let through, got {mentions:?}. \
         {context}"
    );
    assert!(
        value("deletion-error").contains("not started"),
        "the page did not say why the period was not set, got {:?}. {context}",
        value("deletion-error")
    );
}
