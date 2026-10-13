//! The cover while a call is up: the call, instead of the grid.
//!
//! Who, large, near the top; under it how long, or how the call stands
//! until it is connected; their picture behind it all when they have one,
//! and the plain cover with their avatar when they have none. In the
//! middle, a pill for the microphone being off and one for the camera
//! being paused, each only when it is so. One action, the microphone,
//! whose icon is what a tap does and whose tap does it -- never hanging
//! up, never the camera -- and none while the call only rings. The
//! reader's own quick actions are gone for the call and back after it,
//! the grid is not laid out again under it, and the call's clock ticks
//! only while somebody is looking.
//!
//! The call is a stand-in with the real one's names (`calls.rs`), so the
//! cover can be put in each state at will; the probe checks the names
//! against a real `Call` so the two cannot drift apart.

// Qt harness: see qml_cover.rs.
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

/// The probe, with the components directory filled in so it can write the
/// `Settings` the cover reads.
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

        // What the cover reads of the window's call, by the same names.
        QtObject {
            id: fakeCall
            property string state: ''
            property string peer_name: ''
            property string peer_avatar: ''
            property string peer_color: '#3080c0'
            property real connected_at: 0
            property bool muted: false
            property bool camera: false
            property bool has_video: false
            property string mutes: ''
            function mute(on) {
                muted = on
                mutes += (on ? 'off' : 'on') + ';'
            }
        }

        // The real one, to hold the stand-in to its names.
        Call { id: realCall }

        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 240, height: 360, call: fakeCall })
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
            return null
        }
        function allIn(node, name, found) {
            if (!node) { return found }
            if (node.objectName === name) { found.push(node) }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                allIn(kids[i], name, found)
            }
            return found
        }
        // Every name the stand-in has that the real call lacks.
        function missingNames() {
            var names = ['state', 'peer_name', 'peer_avatar', 'peer_color',
                         'connected_at', 'muted', 'camera', 'has_video']
            var missing = []
            for (var i = 0; i < names.length; i++) {
                if (realCall[names[i]] === undefined) { missing.push(names[i]) }
            }
            if (typeof realCall.mute !== 'function') { missing.push('mute()') }
            return missing.join(',')
        }
        function count(n) { Settings.quickActionCount = n; return 'ok' }
        function set(side, kind) {
            var prefix = side === 'right' ? 'quickActionRight' : 'quickActionLeft'
            Settings[prefix] = kind
            Settings[prefix + 'Account'] = 0
            Settings[prefix + 'Chat'] = 0
            Settings[prefix + 'Icon'] = ''
            return 'ok'
        }
        function call(state, name, avatar, connectedAgo, video, camera) {
            fakeCall.state = state
            fakeCall.peer_name = name
            fakeCall.peer_avatar = avatar
            fakeCall.connected_at = connectedAgo >= 0 ? Date.now() / 1000 - connectedAgo : 0
            fakeCall.has_video = video === 'true'
            fakeCall.camera = camera === 'true'
            return 'ok'
        }
        function setMuted(on) { fakeCall.muted = on === 'true'; return 'ok' }
        function setCamera(on) { fakeCall.camera = on === 'true'; return 'ok' }
        function shown(name) {
            var item = findIn(loader.item, name)
            return item ? '' + item.visible : 'missing'
        }
        function text(name) {
            var item = findIn(loader.item, name)
            return item ? '' + item.text : 'missing'
        }
        // What the cover is, as `call|grid`: which of the two is drawn.
        function mode() { return shown('callView') + '|' + shown('avatarGrid') }
        // The picture, the shade over it, and the avatar in its place.
        function backdrop() {
            return shown('callPicture') + '|' + shown('callShade') + '|' + shown('callAvatar')
        }
        // Which pills are up, as `mic|camera`.
        function notes() { return shown('micNote') + '|' + shown('cameraNote') }
        function noteText(name) {
            return '' + findIn(findIn(loader.item, name), 'callNoteText').text
        }
        // The action lists the cover holds, with whether each is on.
        function lists() {
            var kids = loader.item.data
            var out = []
            for (var i = 0; i < kids.length; i++) {
                if (kids[i].actions !== undefined) {
                    out.push(kids[i].objectName + '=' + kids[i].enabled)
                }
            }
            return out.join(';')
        }
        // Every action the home screen would draw now: the actions of
        // every list that is on, by their pictures' file names.
        function drawn() {
            var kids = loader.item.data
            var out = []
            for (var i = 0; i < kids.length; i++) {
                if (kids[i].actions === undefined || !kids[i].enabled) { continue }
                for (var j = 0; j < kids[i].actions.length; j++) {
                    out.push(('' + kids[i].actions[j].iconSource).split('/').pop())
                }
            }
            return out.join(';')
        }
        function micIcon() {
            return '' + findIn(loader.item, 'callActions').actions[0].iconSource
        }
        function tapMic() {
            findIn(loader.item, 'callActions').actions[0].triggered()
            return fakeCall.mutes + '|' + fakeCall.muted
        }
        function look(status) { loader.item.status = status; return 'ok' }
        function ticking() { return '' + findIn(loader.item, 'callClock').running }
        function ink(colour) { Theme.primaryColor = colour; return 'ok' }
        function markUnread(list, chatId) {
            allIn(loader.item, 'coverChats', [])[list].mark_unread(chatId)
            return 'ok'
        }
        function grid() {
            return loader.item.unreadTotal + '|' + (loader.item.stale ? 'stale' : 'fresh')
        }
        // See qml_quick_actions_cover.rs.
        function shaders(node, found) {
            if (!node) { return found }
            var what = ('' + node).split('(')[0]
            if (what.indexOf('QQuickShaderEffect') === 0) { found.push(what) }
            if (node.layer && node.layer.enabled) { found.push('layer:' + what) }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) { shaders(kids[i], found) }
            return found
        }
        function drawnThrough() { return shaders(loader.item, []).join(';') }
    }
";

fn cover_url() -> String {
    format!(
        "file://{}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/cover/CoverPage.qml")
            .display()
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_cover_shows_the_call_while_one_is_up() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-cover-call-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let picture = common::a_real_picture();
    // A file that is there and is not a picture: it never loads.
    let not_a_picture = temp.join("not-a-picture.png");
    std::fs::write(&not_a_picture, b"not a picture").expect("write a broken picture");
    let not_a_picture = not_a_picture.display().to_string();

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them. Two profiles with people in them, so the
    // grid has something to be stale about.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
    }

    piirit_shim::register_qml_types();
    common::register_cover_enum();

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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }
    macro_rules! state {
        ($state:expr, $name:expr, $avatar:expr, $ago:expr, $video:expr, $camera:expr) => {
            call!(
                "call",
                QString::from($state),
                QString::from($name),
                QString::from($avatar),
                $ago,
                QString::from($video),
                QString::from($camera)
            )
        };
    }
    macro_rules! text {
        ($name:expr) => {
            call!("text", QString::from($name))
        };
    }

    // SAFETY: the callbacks fire only while `exec()` runs on this thread.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!("names", call!("missingNames"));
        // One quick action of the reader's own, to see it go and come back.
        record!("count", call!("count", 1));
        record!(
            "set-left",
            call!("set", QString::from("left"), QString::from("search"))
        );
        record!(
            "set-right",
            call!("set", QString::from("right"), QString::from(""))
        );
        record!("load", call!("load", QString::from(cover_url())));
        record!("idle-mode", call!("mode"));
        record!("idle-lists", call!("lists"));
        record!("idle-drawn", call!("drawn"));
    });

    // The lists are in. A video call rings here, from someone without a
    // picture.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("idle-grid", call!("grid"));
        record!("ring", state!("ringing", "Alice", "", -1, "true", "true"));
        record!("ring-mode", call!("mode"));
        record!("ring-name", text!("callName"));
        record!("ring-time", text!("callTime"));
        record!("ring-notes", call!("notes"));
        record!("ring-drawn", call!("drawn"));
        record!("ring-backdrop", call!("backdrop"));

        // Someone writes during the call: the grid waits.
        record!("mark", call!("markUnread", 0, 1));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("ring-grid", call!("grid"));

        // A voice call placed from here, ringing at the other end, and then
        // being set up.
        record!(
            "calling",
            state!("calling", "Bob", "", -1, "false", "false")
        );
        record!("calling-time", text!("callTime"));
        record!("calling-lists", call!("lists"));
        record!("calling-drawn", call!("drawn"));
        record!("calling-notes", call!("notes"));
        record!(
            "connecting",
            state!("connecting", "Bob", "", -1, "false", "false")
        );
        record!("connecting-time", text!("callTime"));

        // Connected a minute and five seconds ago.
        record!(
            "connected",
            state!("connected", "Bob", "", 65, "false", "false")
        );
        record!("connected-time", text!("callTime"));
        record!("connected-ticking", call!("ticking"));
        record!("voice-notes", call!("notes"));
        record!("voice-backdrop", call!("backdrop"));
        record!("live-icon", call!("micIcon"));

        // The microphone, off from the cover, and on again.
        record!("tap-off", call!("tapMic"));
        record!("muted-icon", call!("micIcon"));
        record!("muted-notes", call!("notes"));
        record!(
            "muted-note-text",
            call!("noteText", QString::from("micNote"))
        );
        record!("tap-on", call!("tapMic"));
        record!("unmuted-notes", call!("notes"));
        record!("unmuted-icon", call!("micIcon"));

        // A light ambience gets the black ink.
        record!("light", call!("ink", QString::from("#000000")));
        record!("light-icon", call!("micIcon"));
        record!("dark", call!("ink", QString::from("#ffffff")));

        // Nobody looking: the clock stops. Looked at again: it goes on.
        record!("away", call!("look", 0));
        record!("away-ticking", call!("ticking"));
        record!("back", call!("look", 2));
        record!("back-ticking", call!("ticking"));

        // A video call with the camera wanted, and their picture.
        record!(
            "video",
            state!("connected", "Carol", picture.clone(), 3725, "true", "true")
        );
        record!("video-time", text!("callTime"));
        record!("video-notes", call!("notes"));
        record!(
            "camera-note-text",
            call!("noteText", QString::from("cameraNote"))
        );
        record!("both", call!("setMuted", QString::from("true")));
        record!("both-notes", call!("notes"));
        record!("both-drawn", call!("drawn"));
        record!("unmute", call!("setMuted", QString::from("false")));
        // The camera switched off in a video call: nothing to say of it.
        record!("camera-off", call!("setCamera", QString::from("false")));
        record!("camera-off-notes", call!("notes"));
    });

    // Their picture has loaded off the main thread.
    single_shot(Duration::from_secs(7), move || unsafe {
        record!("pictured-backdrop", call!("backdrop"));
        record!("pictured-shaders", call!("drawnThrough"));
        record!("pictured-name", text!("callName"));
    });

    // The clock has ticked at least once since. Then nobody looks for a
    // while.
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("later-time", text!("callTime"));
        record!("asleep", call!("look", 0));
        record!("asleep-time", text!("callTime"));
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        record!("still-asleep-time", text!("callTime"));
        record!("awake", call!("look", 2));
        record!("awake-time", text!("callTime"));

        // A picture that never loads is no picture.
        record!(
            "broken",
            state!(
                "connected",
                "Dave",
                not_a_picture.clone(),
                10,
                "false",
                "false"
            )
        );
    });

    single_shot(Duration::from_secs(11), move || unsafe {
        record!("broken-backdrop", call!("backdrop"));

        // Over: the grid and the reader's action come back, with what
        // arrived during the call.
        record!("ended", state!("ended", "Dave", "", 10, "false", "false"));
        record!("ended-mode", call!("mode"));
        record!("ended-lists", call!("lists"));
        record!("ended-drawn", call!("drawn"));
        record!("ended-ticking", call!("ticking"));
        record!("ended-grid", call!("grid"));
        record!("reset", state!("", "", "", -1, "false", "false"));
        record!("reset-mode", call!("mode"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps.iter().find(|(name, _)| *name == label).map_or_else(
            || "<step did not run>".to_string(),
            |(_, value)| value.clone(),
        )
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("names"),
        "",
        "the stand-in reads names the real call does not have. {context}"
    );
    assert_eq!(value("load"), "ok", "the cover did not load. {context}");

    // No call: the grid, and the reader's action.
    assert_eq!(value("idle-mode"), "false|true", "{context}");
    assert_eq!(
        value("idle-lists"),
        "callActions=false;twoActions=false;oneAction=true",
        "without a call the cover does not offer the reader's action alone. {context}"
    );
    assert_eq!(value("idle-drawn"), "search-32-white.png", "{context}");

    // Ringing here: the call, with how it stands, and nothing to tap.
    assert_eq!(
        value("ring-mode"),
        "true|false",
        "a ringing call does not take the cover over from the grid. {context}"
    );
    assert_eq!(value("ring-name"), "Alice", "{context}");
    assert_eq!(value("ring-time"), "Incoming video call", "{context}");
    assert_eq!(
        value("ring-notes"),
        "false|false",
        "a call still ringing says something of a camera it has not got. {context}"
    );
    assert_eq!(
        value("ring-drawn"),
        "",
        "a call ringing here has an action on the cover. {context}"
    );
    assert_eq!(
        value("ring-backdrop"),
        "false|false|true",
        "without a picture the cover is not the plain one with their avatar. {context}"
    );

    // The grid is not laid out under the call, and catches up after it.
    assert_eq!(value("idle-grid"), "0|fresh", "{context}");
    assert_eq!(
        value("ring-grid"),
        "0|stale",
        "the grid was laid out again under the call. {context}"
    );
    assert_eq!(
        value("ended-grid"),
        "1|fresh",
        "what arrived during the call is not on the grid after it. {context}"
    );

    // Before it is connected, the call page's words.
    assert_eq!(value("calling-time"), "Ringing…", "{context}");
    assert_eq!(value("connecting-time"), "Connecting…", "{context}");
    assert_eq!(
        value("calling-lists"),
        "callActions=true;twoActions=false;oneAction=false",
        "during a call the cover offers anything but its own action. {context}"
    );
    assert_eq!(
        value("calling-drawn"),
        "mic-off-32-white.png",
        "the one action during a call is not the microphone. {context}"
    );
    assert_eq!(value("calling-notes"), "false|false", "{context}");

    // Connected: how long, ticking.
    let connected = value("connected-time");
    assert!(
        ["00:01:05", "00:01:06", "00:01:07"].contains(&connected.as_str()),
        "a call connected 65 seconds ago does not say so: {connected}. {context}"
    );
    let long = value("video-time");
    assert!(
        long.starts_with("01:02:0"),
        "an hour-long call does not count its hours: {long}. {context}"
    );
    assert_eq!(value("connected-ticking"), "true", "{context}");
    // Carol's call is the same call from 6 s to 10 s: the clock moves on
    // while it is looked at, holds while it is not, and is right again the
    // moment it is looked at again.
    assert_ne!(
        value("later-time"),
        long,
        "the call's clock does not tick while the cover is looked at. {context}"
    );
    assert_eq!(
        value("still-asleep-time"),
        value("asleep-time"),
        "the call's clock moved on while nobody was looking. {context}"
    );
    assert_ne!(
        value("awake-time"),
        value("still-asleep-time"),
        "the call's clock is not brought up to date when looked at again. {context}"
    );
    assert_eq!(
        value("away-ticking"),
        "false",
        "the call's clock ticks on while nobody is looking at the cover. {context}"
    );
    assert_eq!(value("back-ticking"), "true", "{context}");
    assert_eq!(
        value("ended-ticking"),
        "false",
        "the call's clock ticks on after the call. {context}"
    );

    // The pills: each only when it is so.
    assert_eq!(
        value("voice-notes"),
        "false|false",
        "a voice call with the microphone on has something to say. {context}"
    );
    assert_eq!(
        value("muted-notes"),
        "true|false",
        "the microphone being off is not said. {context}"
    );
    assert_eq!(value("muted-note-text"), "Microphone off", "{context}");
    assert_eq!(value("unmuted-notes"), "false|false", "{context}");
    assert_eq!(
        value("video-notes"),
        "false|true",
        "the camera being paused is not said. {context}"
    );
    assert_eq!(value("camera-note-text"), "Camera paused", "{context}");
    assert_eq!(
        value("both-notes"),
        "true|true",
        "both pills are not up together. {context}"
    );
    assert_eq!(
        value("camera-off-notes"),
        "false|false",
        "a camera that is off is said to be paused. {context}"
    );
    assert_eq!(
        value("both-drawn"),
        "mic-32-white.png",
        "a video call has more than the microphone on the cover. {context}"
    );

    // The microphone action: its icon is what a tap does, and a tap does
    // it.
    assert!(
        value("live-icon").ends_with("/art/cover/mic-off-32-white.png"),
        "with the microphone on, the action is not drawn as switching it off. {context}"
    );
    assert_eq!(
        value("tap-off"),
        "off;|true",
        "a tap on the microphone does not switch it off. {context}"
    );
    assert!(
        value("muted-icon").ends_with("/art/cover/mic-32-white.png"),
        "with the microphone off, the action is not drawn as switching it on. {context}"
    );
    assert_eq!(
        value("tap-on"),
        "off;on;|false",
        "a second tap does not switch the microphone on again. {context}"
    );
    assert!(
        value("unmuted-icon").ends_with("/art/cover/mic-off-32-white.png"),
        "{context}"
    );
    assert!(
        value("light-icon").ends_with("/art/cover/mic-off-32-black.png"),
        "a light ambience does not get the black microphone. {context}"
    );
    for label in ["live-icon", "muted-icon", "light-icon"] {
        let url = value(label);
        let path = url.strip_prefix("file://").unwrap_or_else(|| {
            panic!("the home screen is not handed a file URL ({label}: {url}). {context}")
        });
        assert!(
            std::path::Path::new(path).is_file(),
            "{label} names a file that is not there: {path}. {context}"
        );
    }

    // Their picture, behind it all, and no avatar over the name.
    assert_eq!(value("voice-backdrop"), "false|false|true", "{context}");
    assert_eq!(
        value("pictured-backdrop"),
        "true|true|false",
        "their picture is not what the call is drawn over. {context}"
    );
    assert_eq!(value("pictured-name"), "Carol", "{context}");
    assert_eq!(
        value("pictured-shaders"),
        "",
        "the call on the cover is drawn through a program of its own. {context}"
    );
    assert_eq!(
        value("broken-backdrop"),
        "false|false|true",
        "a picture that never loads leaves the cover without the avatar. {context}"
    );

    // Over: the grid and the reader's own action again.
    assert_eq!(
        value("ended-mode"),
        "false|true",
        "the cover stays on a call that is over. {context}"
    );
    assert_eq!(
        value("ended-lists"),
        "callActions=false;twoActions=false;oneAction=true",
        "the reader's action does not come back after the call. {context}"
    );
    assert_eq!(value("ended-drawn"), "search-32-white.png", "{context}");
    assert_eq!(value("reset-mode"), "false|true", "{context}");
}
