//! Own profile, picture and name, top left of chat list: off until switched on in settings.
//! Header and pills sit on accent glow either way.
//! Shows the list's own profile only (core's list holds all), follows a new name or picture,
//! taps through to profiles. Archived list: none (a mode, not where profiles switch).

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    import 'file://__COMPONENTS__'
    Item {
        // Silica `pageStack`, recorded not performed.
        property QtObject pageStack: QtObject {
            property string log: ''
            function push(page, properties) {
                log = log + 'push:' + ('' + page).split('/').pop() + ' '
                    + JSON.stringify(properties) + '|'
            }
        }
        Loader { id: loader }
        // Renames, pictures profile 1 as its own page does.
        Profile { id: me }
        function load(url, accountId, archived) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId, archived: archived })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function all(node, name, found) {
            if (!node) { return found }
            if (node.objectName === name && found.indexOf(node) < 0) { found.push(node) }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) { all(kids[i], name, found) }
            if (node.contentItem && node.contentItem !== node) {
                all(node.contentItem, name, found)
            }
            return found
        }
        function profiles() { return all(loader.item, 'headerProfile', []) }
        function built() { return '' + profiles().length }
        function shownOnes() { return profiles().filter(function (item) { return item.visible }) }
        // Each shown profile: name, picture, colour.
        function shown() {
            return shownOnes().map(function (item) {
                var avatar = all(item, 'headerProfileAvatar', [])[0]
                return all(item, 'headerProfileName', [])[0].text + '|' + avatar.picturePath
                    + '|' + avatar.ownColor
            }).join(',')
        }
        // Left of title, picture centred on title's line, small, name quiet.
        function look() {
            var hit = shownOnes()
            if (hit.length !== 1) { return 'shown:' + hit.length }
            var header = all(loader.item, 'chatListHeader', [])[0]
            var avatar = all(hit[0], 'headerProfileAvatar', [])[0]
            var name = all(hit[0], 'headerProfileName', [])[0]
            return [hit[0].parent.parent === header.extraContent,
                    avatar.y + avatar.height / 2 === header.topMargin + header.titleHeight / 2,
                    // Anchors snap to whole pixels.
                    Math.abs(name.y + name.height / 2 - avatar.y - avatar.height / 2) <= 1,
                    avatar.width === all(loader.item, 'chatSearchPill', [])[0].height,
                    name.font.pixelSize === Theme.fontSizeSmall,
                    Qt.colorEqual(name.color, Theme.secondaryColor)].join(',')
        }
        // Accent glow behind header and pills: from top, still on at their bottom, gone below.
        function glow() {
            var glow = all(loader.item, 'chatListGlow', [])[0]
            if (!glow) { return 'missing' }
            var heading = all(loader.item, 'chatListHeader', [])[0].parent
            var stops = glow.gradient.stops
            var accent = Theme.highlightBackgroundColor
            var tinted = function (color) {
                return Qt.colorEqual(Qt.rgba(color.r, color.g, color.b, 1), accent)
            }
            var kids = glow.parent.children
            var order = glow.z < heading.z || glow.z === heading.z
                && Array.prototype.indexOf.call(kids, glow)
                    < Array.prototype.indexOf.call(kids, heading)
            return [glow.parent === heading.parent && order,
                    glow.y === 0 && glow.width === heading.width,
                    glow.height > heading.height,
                    stops.length === 3,
                    tinted(stops[0].color) && stops[0].color.a > 0,
                    Math.abs(stops[1].position * glow.height - heading.height) < 0.5
                        && tinted(stops[1].color) && stops[1].color.a > 0,
                    stops[2].position === 1 && tinted(stops[2].color)
                        && stops[2].color.a === 0].join(',')
        }
        function tap() {
            var hit = profiles().filter(function (item) { return item.visible })
            if (hit.length !== 1) { return 'shown:' + hit.length }
            hit[0].clicked(null)
            return 'ok'
        }
        function navigation() { return pageStack.log }
        function setShown(on) { Settings.chatListProfile = on; return 'ok' }
        function setting() { return '' + Settings.chatListProfile }
        function settingKey() { return Settings.chatListProfileConfig.key }
        function pick(accountId) { me.account_id = accountId; return 'ok' }
        function loaded() { return '' + me.loaded }
        function rename(name) { me.save(name, ''); return 'ok' }
        function picture(path) { me.set_picture(path); return 'ok' }
        // Profile page's step after a change.
        function refresh() { core.refresh_accounts(); return 'ok' }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_chat_list_shows_its_own_profile_once_asked() {
    let temp = std::env::temp_dir().join(format!("piirit-header-profile-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt and before server inherits.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Two profiles: header must pick the list's.
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    engine.load_data(QByteArray::from(
        PROBE_QML
            .replace("__COMPONENTS__", &components.display().to_string())
            .as_str(),
    ));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);
    let picture = common::a_real_picture();
    let picture_arg = picture.clone();

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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!("key", call!("settingKey"));
        record!("default", call!("setting"));
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1,
                false
            )
        );
    });

    // Off: nothing built, not just hidden.
    plan.after(1, move || unsafe {
        record!("off-built", call!("built"));
        record!("glow", call!("glow"));
        record!("switch-on", call!("setShown", true));
    });

    // Address stands in for a profile with no name, as settings' profile row.
    plan.until(1, 10, move |last| unsafe {
        let shown = call!("shown");
        let done = !shown.is_empty();
        if done || last {
            record!("on-shown", shown);
            record!("pick", call!("pick", 1));
        }
        done
    });

    plan.until(1, 10, move |last| unsafe {
        let done = call!("loaded") == "true";
        if done || last {
            record!("rename", call!("rename", QString::from("Muhn")));
        }
        done
    });
    plan.after(1, move || unsafe {
        record!(
            "picture",
            call!("picture", QString::from(picture_arg.as_str()))
        );
    });
    plan.after(1, move || unsafe {
        record!("refresh", call!("refresh"));
    });
    plan.until(1, 10, move |last| unsafe {
        let shown = call!("shown");
        let done = shown.starts_with("Muhn|");
        if done || last {
            record!("renamed-shown", shown);
            record!("look", call!("look"));
            record!("tap", call!("tap"));
            record!("tap-opens", call!("navigation"));
            record!("switch-off", call!("setShown", false));
            record!("off-again-built", call!("built"));
            record!("glow-on", call!("glow"));
            record!("switch-on-again", call!("setShown", true));
            record!(
                "archived",
                call!(
                    "load",
                    QString::from(common::page_url("ChatListPage.qml")),
                    1,
                    true
                )
            );
        }
        done
    });

    plan.after(1, move || unsafe {
        record!("archived-built", call!("built"));
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
        value("key"),
        "/apps/harbour-piirit/chat_list_profile",
        "{context}"
    );
    assert_eq!(
        value("default"),
        "false",
        "the profile shows over the chat list before anyone asked for it. {context}"
    );
    assert_eq!(value("load"), "ok", "the chat list did not load. {context}");
    assert_eq!(
        value("off-built"),
        "0",
        "with the setting off, the chat list still builds a profile in its header. {context}"
    );
    assert_eq!(
        value("on-shown"),
        "account1@example.org||#4a90d9",
        "switched on, the header does not show this list's profile alone, by address while it \
         has no name. {context}"
    );
    for label in ["switch-on", "pick", "rename", "picture", "refresh"] {
        assert_eq!(value(label), "ok", "{label} failed. {context}");
    }
    assert_eq!(
        value("renamed-shown"),
        format!("Muhn|{picture}|#4a90d9"),
        "the header does not follow the profile's new name and picture. {context}"
    );
    assert_eq!(
        value("look"),
        "true,true,true,true,true,true",
        "the profile is not a picture and quiet name left of the title, on its line \
         (in header's space left of title, picture centred on title, name on picture, \
         picture as tall as search pill, name size, name colour). {context}"
    );
    for label in ["glow", "glow-on"] {
        assert_eq!(
            value(label),
            "true,true,true,true,true,true,true",
            "{label}: the header and pills are not on an accent glow that fades out below them \
             (behind them, page wide from top, reaches past pills, three stops, accent at top, \
             still on at pills' bottom, gone at end). {context}"
        );
    }
    assert_eq!(value("tap"), "ok", "{context}");
    assert_eq!(
        value("tap-opens"),
        r#"push:ProfilesPage.qml {"currentAccountId":1}|"#,
        "tapping the profile does not open profiles, on this one. {context}"
    );
    assert_eq!(
        value("off-again-built"),
        "0",
        "switched off, the profile stays in the header. {context}"
    );
    assert_eq!(
        value("archived-built"),
        "0",
        "the archived list shows the profile too. {context}"
    );
}
