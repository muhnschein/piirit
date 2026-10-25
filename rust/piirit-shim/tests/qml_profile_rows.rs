//! Profile row on profiles page: own picture and colour, way to its page, survives
//! another profile's deletion.
//!
//! Device bug: deleting two at once deleted the first; reload rebuilt rows and the second
//! row's countdown (Silica remorse timer lives on row) went with it. Now: wait is not on
//! row (`PendingRemoval`); list still refreshed in place, not rebuilt (no row flicker when one
//! goes). Measured here: second row is same object after first deletion.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    non_snake_case,
    clippy::borrow_as_ptr,
    clippy::expect_used,
    clippy::needless_pass_by_value
)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica `pageStack`, recorded not performed.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:ProfilePage.qml:accountId=1|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    replaceAbove:
        qt_method!(fn(&mut self, target: QVariant, page: QString, properties: QVariantMap)),
}

impl StackProbe {
    fn push(&mut self, page: QString, properties: QVariantMap) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page);
        let account =
            i32::from_qvariant(properties.value(QString::from("accountId"), QVariant::default()))
                .unwrap_or(0);
        let current = self.log.to_string();
        self.log = format!("{current}push:{name}:accountId={account}|").into();
        self.log_changed();
    }

    fn replaceAbove(&mut self, _target: QVariant, page: QString, _properties: QVariantMap) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page);
        let current = self.log.to_string();
        self.log = format!("{current}replaceAbove:{name}|").into();
        self.log_changed();
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        Loader { id: loader }
        // The first profile, made the way the app makes one, and given a
        // picture the way its page gives it one. The second account is
        // added only once the first is made: both start with add_account,
        // and started together they race for the same id.
        Profile { id: profile; objectName: 'profile' }
        Connections {
            target: core
            onProfile_created: core.add_account()
        }
        // The first profile's chat list, for marking a chat unread the
        // way the chat list's menu does.
        ChatList { id: chats; objectName: 'chats'; account_id: 1 }
        function markUnread() { chats.mark_unread(2); return 'ok' }
        // The badge on a profile's row: whether it shows, and what.
        function badgeOf(accountId) {
            var row = findIn(loader.item, 'profileRow' + accountId)
            if (!row) { return 'missing:profileRow' + accountId }
            var badge = findIn(row, 'profileUnreadBadge')
            var label = findIn(row, 'profileUnreadLabel')
            if (!badge || !label) { return 'missing:profileUnreadBadge' }
            return badge.visible + ':' + label.text
        }
        function seed() {
            core.create_profile('Ada', 'dcaccount:nine.testrun.org')
            return 'ok'
        }
        function givePicture() {
            profile.account_id = 1
            profile.set_picture('/tmp/piirit-fake/ada.jpg')
            return 'ok'
        }
        function refresh() { core.refresh_accounts(); return 'ok' }
        function load(url, currentAccountId) {
            loader.setSource('', {})
            loader.setSource(url, {
                currentAccountId: currentAccountId,
                // The wait before a profile goes, turned down so this
                // does not have to sit through four seconds of it twice.
                pendingDelay: 200
            })
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
            if (node.menu) {
                var inMenu = findIn(node.menu, name)
                if (inMenu) { return inMenu }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        // What the row draws for the profile's picture and colour.
        function avatarOf(accountId, property) {
            var row = findIn(loader.item, 'profileRow' + accountId)
            if (!row) { return 'missing:profileRow' + accountId }
            var avatar = findIn(row, 'contactAvatar')
            if (!avatar) { return 'missing:contactAvatar' }
            return '' + avatar[property]
        }
        function clickIn(accountId, name) {
            var row = findIn(loader.item, 'profileRow' + accountId)
            if (!row) { return 'missing:profileRow' + accountId }
            var item = findIn(row, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        /// Whether the platform's countdown is up over that profile,
        /// and whether it fits inside the row it covers. Silica's own
        /// RemorseItem, not a stand-in.
        function goingOut(accountId) {
            var row = findIn(loader.item, 'profileRow' + accountId)
            if (!row) { return 'missing:profileRow' + accountId }
            var remorse = findIn(row, 'profileRemorse')
            if (!remorse) { return 'missing:profileRemorse' }
            return remorse.active + ':' + (remorse.height <= row.height)
        }
        // The row object for a profile, held to compare identity with.
        property var remembered: null
        function remember(accountId) {
            remembered = findIn(loader.item, 'profileRow' + accountId)
            return remembered ? 'ok' : 'missing:profileRow' + accountId
        }
        function stillThere(accountId) {
            var row = findIn(loader.item, 'profileRow' + accountId)
            if (!row) { return 'gone' }
            return row === remembered ? 'same' : 'rebuilt'
        }
        // How many rows the view holds. A released delegate can linger in
        // the item tree headlessly, so the count is what says a row went.
        function rowCount() {
            var view = findIn(loader.item, 'profileList')
            return view ? '' + view.count : 'missing:profileList'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_profile_row_has_its_picture_leads_to_its_page_and_outlives_a_neighbour() {
    let temp = std::env::temp_dir().join(format!("piirit-profile-rows-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(StackProbe::default());
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!("seed", call!("seed"));
    });
    plan.after(2, move || unsafe {
        record!("picture", call!("givePicture"));
    });
    plan.after(2, move || unsafe {
        call!("refresh");
    });
    plan.after(2, move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("ProfilesPage.qml")),
                1
            )
        );
    });
    plan.after(1, move || unsafe {
        // Nothing unread; then chat marked unread: core announces, row follows.
        record!("badge-before", call!("badgeOf", 1));
        record!("mark-unread", call!("markUnread"));
    });
    plan.after(1, move || unsafe {
        record!("badge-after", call!("badgeOf", 1));
        record!(
            "avatar-path",
            call!("avatarOf", 1, QString::from("picturePath"))
        );
        record!(
            "avatar-color",
            call!("avatarOf", 1, QString::from("ownColor"))
        );
        record!(
            "settings",
            call!("clickIn", 1, QString::from("profileSettingsItem"))
        );
        // Two deletions at once: second row outlives first profile's going.
        record!("remember", call!("remember", 2));
        record!(
            "delete-1",
            call!("clickIn", 1, QString::from("deleteProfileItem"))
        );
        // Read while waiting: the state reader sees for those seconds.
        record!("going-1", call!("goingOut", 1));
    });
    plan.after(3, move || unsafe {
        record!("row-2-after", call!("stillThere", 2));
        record!("rows-after", call!("rowCount"));
        record!(
            "delete-2",
            call!("clickIn", 2, QString::from("deleteProfileItem"))
        );
    });
    plan.after(3, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_rows(&steps, &navigation, &common::calls(&journal));
}

/// Picture, colour from core; badge follows unread; menu opens page; second row same object
/// after first deletion.
fn assert_rows(steps: &[(&str, String)], navigation: &str, calls: &[(String, Value)]) {
    let context = format!("steps: {steps:?}\nnavigation: {navigation}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    for label in [
        "seed",
        "picture",
        "load",
        "mark-unread",
        "settings",
        "remember",
        "delete-1",
        "delete-2",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert_eq!(
        value("badge-before"),
        "false:0",
        "the row shows a badge with nothing unread. {context}"
    );
    assert_eq!(
        value("badge-after"),
        "true:1",
        "a chat marked unread did not put a badge on the profile's row. {context}"
    );
    assert_eq!(
        value("avatar-path"),
        "/tmp/piirit-fake/ada.jpg",
        "the row does not draw the profile's own picture. {context}"
    );
    assert_eq!(
        value("avatar-color"),
        "#4a90d9",
        "the row does not use the colour the core gives the profile. {context}"
    );
    assert!(
        navigation.contains("push:ProfilePage.qml:accountId=1|"),
        "the row's menu did not open the profile's page with its id. {context}"
    );
    assert_eq!(
        value("going-1"),
        "true:true",
        "the platform's countdown is not up over a profile waiting to go, \
         or it is taller than the row it covers. {context}"
    );

    assert_eq!(
        value("rows-after"),
        "1",
        "the deleted profile's row is still in the list, so nothing was refreshed. {context}"
    );
    assert_eq!(
        value("row-2-after"),
        "same",
        "the first deletion rebuilt the other profile's row, and with it \
         whatever that row was counting down to. {context}"
    );
    let removed: Vec<u64> = calls
        .iter()
        .filter(|(name, _)| name == "remove_account")
        .filter_map(|(_, params)| params.pointer("/0").and_then(Value::as_u64))
        .collect();
    assert_eq!(
        removed,
        vec![1, 2],
        "both deletions should have reached the core, in order: {calls:?}"
    );
}
