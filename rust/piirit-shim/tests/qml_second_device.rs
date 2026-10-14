//! Offer one profile to a second device, from its page (other end of `qml_restore.rs`:
//! that reads a code, this shows it). Core provider is per profile (takes account id), so
//! page is reached from profile's row. Pinned:
//!
//! - offer is `provide_backup` on this account, `get_backup_qr` for code text; code drawn
//!   and printed is core's answer;
//! - nothing goes up alone: code waits behind button, "Show code" then "Show code again";
//! - going back ends offer in core; only a transfer under way pins page. A page pushed
//!   over (a call's) is not leaving: offer stays up;
//! - Cancel stops provider in core, reader stays on page with button back;
//! - offer reader stops reports nothing (core refuses provider it was told to stop);
//! - core without code: offer stopped, reason in core's words;
//! - no taker is not a hand-over: both `Ok`, only progress differs (`second_device.rs`);
//! - device taking profile ends page: code goes, chats attached right (way on is a swipe).
//!
//! One profile per offer: provider and fake server keyed only on account
//! (`fake_core_server.rs`): 3 no code, 2 refused, 4/6 nobody comes (Cancel / back),
//! 1 device stays, 5 device starts and leaves.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    // `pushAttached` is Silica's name; probe must answer to it.
    non_snake_case,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // Must match dispatcher's by-value params (piirit-shim/src/lib.rs).
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica's `pageStack`, recorded. Chats attached right; Cancel leaves none.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `attach:ChatListPage.qml:9|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    /// Silica's name for pushing right.
    pushAttached: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    pop: qt_method!(fn(&mut self)),
}

impl PageStackProbe {
    fn push(&mut self, page: QString, _properties: QVariantMap) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        self.note(&format!("push:{name}"));
    }

    fn pushAttached(&mut self, page: QString, properties: QVariantMap) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let account = properties.value(QString::from("accountId"), QVariant::default());
        let account = i32::from_qvariant(account).unwrap_or_default();
        self.note(&format!("attach:{name}:{account}"));
    }

    fn pop(&mut self) {
        self.note("pop:");
    }

    fn note(&mut self, what: &str) {
        let current = self.log.to_string();
        self.log = format!("{current}{what}|").into();
        self.log_changed();
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        // `currentAccountId` is the profile the chats are on, which is
        // not the one being offered: the page attaches that chat list
        // to the right once a device has taken the profile.
        function load(url, accountId, currentAccountId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                currentAccountId: currentAccountId
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // Whose profile, off the row at the top: drawn the way the
        // invite code's page draws it.
        function profileShows(property) {
            var row = findIn(loader.item, 'profileRow')
            if (!row) { return 'missing:profileRow' }
            if (property === 'ownColor') {
                var avatar = findIn(row, 'contactAvatar')
                return avatar ? '' + avatar.ownColor : 'missing:contactAvatar'
            }
            return '' + row[property]
        }
        // `data` rather than `children`: the SecondDevice and QrCode
        // objects are plain QObjects and are not among an Item's visual
        // children at all.
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
        function pageProperty(property) { return '' + loader.item[property] }
        // What Silica does to a page when another is pushed over it --
        // a call's, which comes in whatever is on screen -- and back.
        function cover(on) {
            loader.item.status = on ? PageStatus.Deactivating : PageStatus.Active
            return 'ok'
        }
        // The page gone, as going back takes it.
        function leave() {
            loader.setSource('', {})
            return loader.item === null ? 'ok' : 'still-there'
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_second_device_page_offers_one_profile_and_hands_it_over() {
    let temp = std::env::temp_dir().join(format!("piirit-second-device-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt init and server spawn.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Six profiles: provider keyed by account.
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2,3,4,5,6");
        // Device turns up: progress step after first wait, taken after second.
        std::env::set_var("PIIRIT_FAKE_TAKEN", "1");
        std::env::set_var("PIIRIT_FAKE_SLOW_MS", "4000");
        // Refused at offer, or offered with no code ever.
        std::env::set_var("PIIRIT_FAKE_PROVIDE_FAIL", "2");
        std::env::set_var("PIIRIT_FAKE_NO_QR", "3");
        // Device starts, leaves: ends like hand-over.
        std::env::set_var("PIIRIT_FAKE_STALLED", "5");
    }

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

    // No code; chats on other profile: offering isn't switching.
    // Offers start with a press; nothing goes up alone.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load-no-code",
            call!(
                "load",
                QString::from(common::page_url("SecondDevicePage.qml")),
                3,
                9
            )
        );
        record!("idle-code", get!("device", "code"));
        record!("idle-running", get!("device", "running"));
        record!("first-label", get!("showButton", "text"));
        record!("first-press", call!("click", QString::from("showButton")));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!(
            "no-code-said",
            call!("pageProperty", QString::from("errorMessage"))
        );
        record!("no-code-idle", get!("device", "running"));
        record!("no-code-button", get!("showButton", "visible"));
        // No code went up: nothing to show again.
        record!("no-code-label", get!("showButton", "text"));
        // Core refuses to offer.
        record!(
            "load-refused",
            call!(
                "load",
                QString::from(common::page_url("SecondDevicePage.qml")),
                2,
                9
            )
        );
        record!("press-refused", call!("click", QString::from("showButton")));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!(
            "refused-said",
            call!("pageProperty", QString::from("errorMessage"))
        );
        record!("refused-idle", get!("device", "running"));
        record!("refused-code", get!("device", "code"));
        // Nobody comes: code left up until given up.
        record!(
            "load-waiting",
            call!(
                "load",
                QString::from(common::page_url("SecondDevicePage.qml")),
                4,
                9
            )
        );
        record!("press-waiting", call!("click", QString::from("showButton")));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        // Drawn like invite code, account colour.
        record!("colour", call!("profileShows", QString::from("ownColor")));
        record!("waiting-code", get!("device", "code"));
        record!("waiting-shown", get!("deviceQr", "visible"));
        record!("waiting-size", get!("qr", "size"));
        record!("waiting-text", get!("codeLabel", "text"));
        record!("waiting-permille", get!("device", "permille"));
        record!("waiting-bar", get!("transferProgress", "visible"));
        record!(
            "waiting-pinned",
            call!("pageProperty", QString::from("backNavigation"))
        );
        record!("cancel", call!("click", QString::from("cancelButton")));
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("cancelled-idle", get!("device", "running"));
        record!("cancelled-code", get!("device", "code"));
        record!(
            "cancelled-said",
            call!("pageProperty", QString::from("errorMessage"))
        );
        record!(
            "cancelled-leavable",
            call!("pageProperty", QString::from("backNavigation"))
        );
        record!("cancelled-button", get!("showButton", "visible"));
        // Code on screen: button says so.
        record!("cancelled-label", get!("showButton", "text"));
        // Back, not Cancel: provider must end either way.
        record!(
            "load-leaving",
            call!(
                "load",
                QString::from(common::page_url("SecondDevicePage.qml")),
                6,
                9
            )
        );
        record!("press-leaving", call!("click", QString::from("showButton")));
    });

    single_shot(Duration::from_secs(11), move || unsafe {
        record!("leaving-code", get!("device", "code"));
        // Covered, not left: offer stays up under page.
        call!("cover", true);
        record!("covered-running", get!("device", "running"));
        record!("covered-code", get!("device", "code"));
        call!("cover", false);
        record!("leave", call!("leave"));
    });

    single_shot(Duration::from_secs(13), move || unsafe {
        // Device turns up and stays.
        record!(
            "load-taken",
            call!(
                "load",
                QString::from(common::page_url("SecondDevicePage.qml")),
                1,
                9
            )
        );
        record!("press-taken", call!("click", QString::from("showButton")));
    });

    single_shot(Duration::from_secs(17), move || unsafe {
        // Mid-transfer: bar up, no way off.
        record!("transfer-permille", get!("device", "permille"));
        record!("transfer-bar", get!("transferProgress", "visible"));
        record!("transfer-code-shown", get!("deviceQr", "visible"));
        record!(
            "transfer-pinned",
            call!("pageProperty", QString::from("backNavigation"))
        );
    });

    single_shot(Duration::from_secs(21), move || unsafe {
        record!("taken", call!("pageProperty", QString::from("taken")));
        record!("taken-said", get!("takenLabel", "visible"));
        record!("taken-idle", get!("device", "running"));
        record!("taken-code", get!("device", "code"));
        record!("taken-button", get!("showButton", "visible"));
        record!("onward", get!("onwardHint", "visible"));
        record!(
            "taken-leavable",
            call!("pageProperty", QString::from("backNavigation"))
        );
        // Device starts, leaves: only progress tells apart.
        record!(
            "load-stalled",
            call!(
                "load",
                QString::from(common::page_url("SecondDevicePage.qml")),
                5,
                9
            )
        );
        record!("press-stalled", call!("click", QString::from("showButton")));
    });

    single_shot(Duration::from_secs(26), move || unsafe {
        record!(
            "stalled-taken",
            call!("pageProperty", QString::from("taken"))
        );
        record!("stalled-idle", get!("device", "running"));
        record!(
            "stalled-said",
            call!("pageProperty", QString::from("errorMessage"))
        );
        record!("stalled-button", get!("showButton", "visible"));
        record!("stalled-label", get!("showButton", "text"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let popped = stack_box.pinned().borrow().log.to_string();
    let calls = common::calls(&journal);
    let context = format!("steps: {steps:?}\nstack: {popped}\ncalls: {calls:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    assert_eq!(
        (
            value("load-no-code").as_str(),
            value("load-refused").as_str(),
            value("load-waiting").as_str(),
            value("load-leaving").as_str(),
            value("load-taken").as_str(),
            value("load-stalled").as_str()
        ),
        ("ok", "ok", "ok", "ok", "ok", "ok"),
        "the page did not load. {context}"
    );
    assert_eq!(
        (
            value("first-press").as_str(),
            value("press-refused").as_str(),
            value("press-waiting").as_str(),
            value("press-leaving").as_str(),
            value("press-taken").as_str(),
            value("press-stalled").as_str()
        ),
        ("ok", "ok", "ok", "ok", "ok", "ok"),
        "an offer was not started by the button. {context}"
    );

    // Going in: nothing goes up; button says what it does.
    assert_eq!(
        (value("idle-code").as_str(), value("idle-running").as_str()),
        ("", "false"),
        "the page put an offer up on its own, before the reader asked \
         for one. {context}"
    );
    assert_eq!(
        value("first-label"),
        "Show code",
        "the button on a page that has never shown a code offers to \
         show one *again*. {context}"
    );

    assert_eq!(
        value("colour"),
        "#00875a",
        "the page does not say whose profile is being offered. {context}"
    );

    // Core can't make code: reason in core's words, nothing left running, button to retry.
    assert!(
        value("no-code-said").contains("no backup provider is running"),
        "a core that would not show a code did not say so in its own \
         words. {context}"
    );
    assert_eq!(
        value("no-code-idle"),
        "false",
        "an offer with no code to show was left running. {context}"
    );
    assert_eq!(
        value("no-code-button"),
        "true",
        "a failed offer left nothing to try again with. {context}"
    );
    assert_eq!(
        value("no-code-label"),
        "Show code",
        "an offer that never got as far as a code left the button \
         offering to show one again. {context}"
    );

    // Provider core refuses.
    assert!(
        value("refused-said").contains("backup could not be offered"),
        "a refused offer did not say so in the core's words. {context}"
    );
    assert_eq!(
        (
            value("refused-idle").as_str(),
            value("refused-code").as_str()
        ),
        ("false", ""),
        "a refused offer left something running or a code on the page. \
         {context}"
    );

    // Code up: drawn, printed, page pinned behind.
    assert!(
        value("waiting-code").starts_with("DCBACKUP"),
        "the page is not showing the code the core answered with: {}. \
         {context}",
        value("waiting-code")
    );
    assert_eq!(
        value("waiting-text"),
        value("waiting-code"),
        "the code printed for a camera that will not read the picture is \
         not the code the picture carries. {context}"
    );
    assert_eq!(
        value("waiting-shown"),
        "true",
        "the code was not drawn. {context}"
    );
    assert!(
        value("waiting-size").parse::<u32>().unwrap_or(0) > 0,
        "the code was not encoded, so the picture is empty. {context}"
    );
    assert_eq!(
        (
            value("waiting-permille").as_str(),
            value("waiting-bar").as_str(),
            value("waiting-pinned").as_str()
        ),
        ("0", "false", "true"),
        "a code nobody has read yet should be a code, not a bar, and the \
         page should be leavable behind it -- the reader has to be able \
         to walk away from their own code. {context}"
    );

    // Going back ends offer in core, like Cancel: provider left running holds profile out
    // with nothing on screen to stop it.
    assert!(
        value("leaving-code").starts_with("DCBACKUP"),
        "there was no offer up to leave: {}. {context}",
        value("leaving-code")
    );
    assert_eq!(
        (
            value("covered-running").as_str(),
            value("covered-code") == value("leaving-code")
        ),
        ("true", true),
        "a page pushed over the offer -- a call's -- ended it, which drops \
         a second device part-way through copying the profile. {context}"
    );
    // Going back stopped it: `assert_offers`'s job.
    assert_eq!(value("leave"), "ok", "nothing left the page. {context}");

    // Reader stop: nothing reported, nothing left up.
    assert_eq!(value("cancel"), "ok", "nothing cancelled. {context}");
    assert!(
        !popped.contains("pop:"),
        "Cancel took the reader off the page: it takes the code down, \
         and the page is where the button is. {context}"
    );
    assert_eq!(
        (
            value("cancelled-idle").as_str(),
            value("cancelled-code").as_str(),
            value("cancelled-said").as_str(),
            value("cancelled-leavable").as_str()
        ),
        ("false", "", "", "true"),
        "an offer the reader stopped was reported back to them, or left \
         a code up, or left the page pinned. {context}"
    );
    // Cancel takes code down; reader stays on button, now "Show code again".
    assert_eq!(
        (
            value("cancelled-button").as_str(),
            value("cancelled-label").as_str()
        ),
        ("true", "Show code again"),
        "after Cancel the reader should be left on the page with the \
         button back, saying it can show the code again. {context}"
    );

    // Device leaves: `Ok`, none of hand-over's words.
    assert_eq!(
        value("stalled-taken"),
        "false",
        "an offer nothing took was reported as a profile handed over, \
         which is the one thing this must never get wrong. {context}"
    );
    assert_eq!(
        value("stalled-idle"),
        "false",
        "an offer that ended was left running. {context}"
    );
    assert!(
        value("stalled-said").contains("No device copied the profile"),
        "an offer nothing took said nothing about it: {}. {context}",
        value("stalled-said")
    );
    assert_eq!(
        (
            value("stalled-button").as_str(),
            value("stalled-label").as_str()
        ),
        ("true", "Show code again"),
        "an offer nothing took left nothing to show the code again \
         with. {context}"
    );

    // Device takes profile: transfer reported; page over.
    assert!(
        value("transfer-permille").parse::<u32>().unwrap_or(0) >= 300,
        "the transfer never reported itself, so the bar sat at nothing \
         the whole way. {context}"
    );
    assert_eq!(
        (
            value("transfer-bar").as_str(),
            value("transfer-code-shown").as_str(),
            value("transfer-pinned").as_str()
        ),
        ("true", "false", "false"),
        "mid-transfer the page should be a bar and no way off it: a \
         swipe is too easy a way to drop a second device half-way \
         through copying the profile. {context}"
    );
    assert_eq!(
        (
            value("taken").as_str(),
            value("taken-said").as_str(),
            value("taken-idle").as_str(),
            value("taken-code").as_str()
        ),
        ("true", "true", "false", ""),
        "a profile handed over did not end the page. {context}"
    );
    assert_eq!(
        value("taken-button"),
        "false",
        "a profile already handed over still offers to show the code \
         again. {context}"
    );
    assert_eq!(
        value("taken-leavable"),
        "true",
        "the page is still pinned once the profile is handed over. \
         {context}"
    );
    assert_eq!(
        value("onward"),
        "true",
        "nothing says there is anywhere to swipe on to. {context}"
    );
    assert!(
        popped.contains("attach:ChatListPage.qml:9|"),
        "the chats were not attached to the right, so the swipe the page \
         offers goes nowhere. {context}"
    );

    assert_offers(&calls, &context);
}

/// Offers: `provide_backup` on page's profile, `get_backup_qr` beside.
/// No-code, cancelled, walked-away offers stopped in core, not left running.
fn assert_offers(calls: &[(String, Value)], context: &str) {
    let accounts = |method: &str| -> Vec<u64> {
        calls
            .iter()
            .filter(|(name, _)| name == method)
            .filter_map(|(_, params)| params.get(0).and_then(Value::as_u64))
            .collect()
    };
    assert_eq!(
        accounts("provide_backup"),
        vec![3, 2, 4, 6, 1, 5],
        "the offers, in order, each on the profile the page was given. \
         {context}"
    );
    assert_eq!(
        accounts("get_backup_qr"),
        vec![3, 2, 4, 6, 1, 5],
        "the code was not asked for beside every offer, on the same \
         profile. {context}"
    );
    assert_eq!(
        accounts("stop_ongoing_process"),
        vec![3, 4, 6],
        "the offers that ended here -- the one with no code to show, the \
         one cancelled and the one walked away from -- were not stopped \
         in the core. {context}"
    );
}
