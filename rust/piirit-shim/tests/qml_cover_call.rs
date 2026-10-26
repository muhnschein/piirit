//! Cover while call up, not grid. Who, large, near top; under it duration, or state until
//! connected. Their picture behind it all, else plain cover with avatar. Pills: mic off, camera
//! paused, each only when so. One action, mic: icon is what a tap does, tap does it -- never hangs
//! up, never camera; none while only ringing. Reader's own quick actions gone for call, back after.
//! Grid not re-laid out under it; call clock ticks only when looked at.
//!
//! Call is a stand-in with real one's names (`calls.rs`), so cover can be put in any state; probe
//! checks names against a real `Call` so the two can't drift.

// Qt harness: see qml_cover.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Probe; components dir filled in for `Settings`.
fn probe_qml() -> String {
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    import QtQuick.Window 2.0
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

        // A shown window, so the cover can be grabbed to a picture of it;
        // on the cover's ground, more or less, so its words read in it.
        Window {
            id: shotWindow
            visible: true
            width: 240
            height: 360
            Rectangle {
                id: shotFrame
                width: loader.item ? loader.item.width : 0
                height: loader.item ? loader.item.height : 0
                color: '#5c4636'
                Loader { id: loader }
            }
        }
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
            // The state last, as the real call sets it: the cover reads
            // the rest as soon as the state says a call is up, and a clock
            // started before connected_at is set counts from 1970.
            fakeCall.peer_name = name
            fakeCall.peer_avatar = avatar
            fakeCall.connected_at = connectedAgo >= 0 ? Date.now() / 1000 - connectedAgo : 0
            fakeCall.has_video = video === 'true'
            fakeCall.camera = camera === 'true'
            fakeCall.state = state
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

        // The theme's sizes as a phone has them, at a pixel ratio of one:
        // the stub's own are only there to resolve, and how the call fits
        // the cover is a matter of how large the words are next to it.
        function phoneTheme() {
            Theme.paddingSmall = 6
            Theme.paddingMedium = 12
            Theme.paddingLarge = 24
            Theme.fontSizeTiny = 20
            Theme.fontSizeExtraSmall = 24
            Theme.fontSizeSmall = 28
            Theme.fontSizeMedium = 32
            Theme.fontSizeLarge = 40
            Theme.fontSizeExtraLarge = 50
            Theme.iconSizeSmall = 32
            Theme.iconSizeMedium = 64
            Theme.itemSizeExtraSmall = 70
            Theme.itemSizeSmall = 80
            Theme.itemSizeMedium = 100
            Theme.coverSizeLarge = Qt.size(234, 374)
            return 'ok'
        }
        function size(w, h) {
            loader.item.width = w
            loader.item.height = h
            shotWindow.width = w
            shotWindow.height = h
            return 'ok'
        }
        function setName(name) { fakeCall.peer_name = name; return 'ok' }
        // Where an item is drawn on the cover, scaled or not.
        function box(item) {
            var from = item.mapToItem(loader.item, 0, 0)
            var to = item.mapToItem(loader.item, item.width, item.height)
            return { x: from.x, y: from.y, r: to.x, b: to.y }
        }
        function label(item) {
            return item.objectName !== '' ? item.objectName : ('' + item).split('(')[0]
        }
        // Everything drawn of the call that is outside the room it has: in
        // from the side margins, and above the strip the home screen draws
        // the action in. The picture and its shade fill the cover on
        // purpose; everything else -- every piece, not only the outer
        // ones -- has to keep in. Words wider or taller than their own box
        // run out of it too.
        function strays(node, room, found) {
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var kid = kids[i]
                if (!kid.visible || kid.objectName === 'callPicture'
                        || kid.objectName === 'callShade') { continue }
                if (kid.width > 0 && kid.height > 0) {
                    var at = box(kid)
                    if (at.x < room.x - 0.5 || at.r > room.r + 0.5
                            || at.y < room.y - 0.5 || at.b > room.b + 0.5) {
                        found.push(label(kid) + '@' + Math.round(at.x) + ',' + Math.round(at.y)
                                   + '-' + Math.round(at.r) + ',' + Math.round(at.b))
                    }
                }
                if (kid.contentWidth !== undefined && kid.lineCount !== undefined
                        && (kid.contentWidth > kid.width + 0.5
                            || kid.contentHeight > kid.height + 0.5)) {
                    found.push(label(kid) + ':spills')
                }
                strays(kid, room, found)
            }
            return found
        }
        // The pieces that must not be drawn over one another.
        function crowding() {
            var names = ['callAvatar', 'callName', 'callTime', 'micNote', 'cameraNote']
            var boxes = []
            for (var i = 0; i < names.length; i++) {
                var item = findIn(loader.item, names[i])
                if (item && item.visible) { boxes.push({ name: names[i], at: box(item) }) }
            }
            var found = []
            for (var a = 0; a < boxes.length; a++) {
                for (var b = a + 1; b < boxes.length; b++) {
                    var one = boxes[a].at
                    var other = boxes[b].at
                    if (one.x < other.r - 0.5 && other.x < one.r - 0.5
                            && one.y < other.b - 0.5 && other.y < one.b - 0.5) {
                        found.push(boxes[a].name + '/' + boxes[b].name)
                    }
                }
            }
            return found
        }
        // How the call is laid out, as `key=value` pairs: what is drawn out
        // of bounds or over something else (nothing, for a good layout),
        // whether the avatar is up, how many lines the name takes, and how
        // large the name, the time and the pills' words are, scaled as
        // drawn.
        function layout() {
            var cover = loader.item
            // A column places its pieces before the next frame rather than
            // at once; the cover is read here as that frame would draw it.
            for (var pass = 0; pass < 3; pass++) {
                findIn(cover, 'callNotes').forceLayout()
                findIn(cover, 'callHead').forceLayout()
            }
            var room = { x: Theme.paddingMedium, r: cover.width - Theme.paddingMedium,
                         y: 0, b: cover.height - cover.actionStrip }
            var name = findIn(cover, 'callName')
            var time = findIn(cover, 'callTime')
            var body = findIn(cover, 'callBody')
            var scale = body ? body.scale : 1
            var note = findIn(findIn(cover, 'micNote'), 'callNoteText')
            return 'strays=' + strays(findIn(cover, 'callView'), room, []).join(',')
                + '|crowded=' + crowding().join(',')
                + '|avatar=' + shown('callAvatar')
                + '|picture=' + shown('callPicture')
                + '|lines=' + name.lineCount
                + '|name=' + Math.round(name.font.pixelSize * scale)
                + '|time=' + Math.round(time.font.pixelSize * scale)
                + '|note=' + Math.round(note.font.pixelSize * scale)
        }
        // The cover as it is drawn, to a file, once the scene has a frame.
        property string saved: ''
        function savedShots() { return saved }
        function shoot(path) {
            var ok = shotFrame.grabToImage(function (result) {
                if (result.saveToFile(path)) { saved += path + ';' }
            })
            return ok ? 'ok' : 'no-grab'
        }
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
    // Not a picture: never loads.
    let not_a_picture = temp.join("not-a-picture.png");
    std::fs::write(&not_a_picture, b"not a picture").expect("write a broken picture");
    let not_a_picture = not_a_picture.display().to_string();

    // SAFETY: single thread; pre-Qt, pre-fork. Two profiles with people: grid can be stale.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        // Probe window needs a scene graph to grab cover from; software one needs no GL.
        std::env::set_var("QT_QUICK_BACKEND", "software");
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

    let mut plan = common::Plan::default();

    // SAFETY: callbacks run only inside `exec()`.
    plan.after(1, move || unsafe {
        record!("names", call!("missingNames"));
        // Own quick action: goes, comes back.
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

    // Lists in. Video call rings, from someone without picture.
    plan.after(3, move || unsafe {
        record!("idle-grid", call!("grid"));
        record!("ring", state!("ringing", "Alice", "", -1, "true", "true"));
        record!("ring-mode", call!("mode"));
        record!("ring-name", text!("callName"));
        record!("ring-time", text!("callTime"));
        record!("ring-notes", call!("notes"));
        record!("ring-drawn", call!("drawn"));
        record!("ring-backdrop", call!("backdrop"));

        // Writes during call: grid waits.
        record!("mark", call!("markUnread", 0, 1));
    });

    plan.after(2, move || unsafe {
        record!("ring-grid", call!("grid"));

        // Voice call placed: ringing, then set up.
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

        // Connected 65 s ago.
        record!(
            "connected",
            state!("connected", "Bob", "", 65, "false", "false")
        );
        record!("connected-time", text!("callTime"));
        record!("connected-ticking", call!("ticking"));
        record!("voice-notes", call!("notes"));
        record!("voice-backdrop", call!("backdrop"));
        record!("live-icon", call!("micIcon"));

        // Mic off, then on, from cover.
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

        // Light ambience: black ink.
        record!("light", call!("ink", QString::from("#000000")));
        record!("light-icon", call!("micIcon"));
        record!("dark", call!("ink", QString::from("#ffffff")));

        // Nobody looking: clock stops; looked at: goes on.
        record!("away", call!("look", 0));
        record!("away-ticking", call!("ticking"));
        record!("back", call!("look", 2));
        record!("back-ticking", call!("ticking"));

        // Video call, camera wanted, their picture.
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
        // Camera off in video call: nothing to say.
        record!("camera-off", call!("setCamera", QString::from("false")));
        record!("camera-off-notes", call!("notes"));
    });

    // Their picture loads off main thread: read once in, however slow the load.
    plan.until(1, 30, move |last| unsafe {
        if !call!("backdrop").starts_with("true|") && !last {
            return false;
        }
        record!("pictured-backdrop", call!("backdrop"));
        record!("pictured-shaders", call!("drawnThrough"));
        record!("pictured-name", text!("callName"));
        true
    });

    // Clock ticked since; then nobody looks a while.
    plan.after(1, move || unsafe {
        record!("later-time", text!("callTime"));
        record!("asleep", call!("look", 0));
        record!("asleep-time", text!("callTime"));
    });

    plan.after(2, move || unsafe {
        record!("still-asleep-time", text!("callTime"));
        record!("awake", call!("look", 2));
        record!("awake-time", text!("callTime"));

        // Never-loading picture is no picture.
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

    plan.after(1, move || unsafe {
        record!("broken-backdrop", call!("backdrop"));

        // Over: grid and action back, arrivals included.
        record!("ended", state!("ended", "Dave", "", 10, "false", "false"));
        record!("ended-mode", call!("mode"));
        record!("ended-lists", call!("lists"));
        record!("ended-drawn", call!("drawn"));
        record!("ended-ticking", call!("ticking"));
        record!("ended-grid", call!("grid"));
        record!("reset", state!("", "", "", -1, "false", "false"));
        record!("reset-mode", call!("mode"));
    });

    // Call fits cover at phone theme sizes: worst name, no picture, both pills; large and
    // small. Each grabbed on own tick (grab lands next frame).
    let picture_dir = std::env::var_os("PIIRIT_COVER_SHOTS")
        .map_or_else(|| temp.join("shots"), std::path::PathBuf::from);
    std::fs::create_dir_all(&picture_dir).expect("create the shots directory");
    let picture_at = move |name: &str| QString::from(picture_dir.join(name).display().to_string());
    let long_name = "Maximiliane Schwarzenberg-Hohenlohe";
    let short_name = "Horst Horstensen";
    macro_rules! case {
        ($label:expr, $name:expr, $avatar:expr, $video:expr, $muted:expr, $camera:expr) => {{
            state!("connected", $name, $avatar, 12, $video, $camera);
            call!("setMuted", QString::from($muted));
            record!($label, call!("layout"));
        }};
    }

    let to_file = picture_at.clone();
    plan.after(1, move || unsafe {
        record!("phone-theme", call!("phoneTheme"));
        record!("large", call!("size", 234, 374));
        // Unconnected, ringing: nothing to switch.
        state!("ringing", long_name, "", -1, "true", "true");
        record!("fit-ringing", call!("layout"));
        state!("calling", long_name, "", -1, "true", "true");
        record!("fit-calling", call!("layout"));
        case!("fit-voice-bare", short_name, "", "false", "true", "false");
        case!(
            "fit-short-bare-mic",
            short_name,
            "",
            "true",
            "true",
            "false"
        );
        case!(
            "fit-short-bare-camera",
            short_name,
            "",
            "true",
            "false",
            "true"
        );
        case!("fit-long-bare-both", long_name, "", "true", "true", "true");
        record!(
            "shot-long-bare",
            call!("shoot", to_file("long-bare-both.png"))
        );
    });

    let to_file = picture_at.clone();
    plan.after(1, move || unsafe {
        case!(
            "fit-short-bare-both",
            short_name,
            "",
            "true",
            "true",
            "true"
        );
        record!(
            "shot-short-bare",
            call!("shoot", to_file("short-bare-both.png"))
        );
    });

    let to_file = picture_at.clone();
    let pictured = common::a_real_picture();
    plan.after(1, move || unsafe {
        record!("small", call!("size", 148, 237));
        case!(
            "fit-small-short-bare",
            short_name,
            "",
            "false",
            "true",
            "false"
        );
        case!(
            "fit-small-long-bare-both",
            long_name,
            "",
            "true",
            "true",
            "true"
        );
        record!(
            "shot-small",
            call!("shoot", to_file("small-long-bare-both.png"))
        );
    });

    // Back to large cover; their picture, off main thread.
    plan.after(1, move || unsafe {
        call!("size", 234, 374);
        state!("connected", long_name, pictured, 12, "true", "true");
    });

    // Read once in, however slow the load.
    let to_file = picture_at.clone();
    plan.until(1, 30, move |last| unsafe {
        let layout = call!("layout");
        if !layout.contains("|picture=true") && !last {
            return false;
        }
        record!("fit-long-pictured-both", layout);
        record!(
            "shot-long-pictured",
            call!("shoot", to_file("long-pictured-both.png"))
        );
        true
    });

    // Voice call, mic off: same picture stays loaded.
    let to_file = picture_at;
    plan.after(1, move || unsafe {
        call!("setName", QString::from(short_name));
        call!("setCamera", QString::from("false"));
        record!("fit-voice-pictured", call!("layout"));
        record!(
            "shot-voice-pictured",
            call!("shoot", to_file("voice-pictured-mic.png"))
        );
    });

    plan.after(1, move || unsafe {
        record!("saved", call!("savedShots"));
        (*engine_ptr).quit();
    });

    plan.run();
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

    // No call: grid and action.
    assert_eq!(value("idle-mode"), "false|true", "{context}");
    assert_eq!(
        value("idle-lists"),
        "callActions=false;twoActions=false;oneAction=true",
        "without a call the cover does not offer the reader's action alone. {context}"
    );
    assert_eq!(value("idle-drawn"), "search-32-white.png", "{context}");

    // Ringing here: call state, nothing to tap.
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

    // Grid laid out after call, not under it.
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

    // Unconnected: call page's words.
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

    // Connected: duration ticking.
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
    // Carol's call, 6 s to 10 s: clock moves when looked at, holds when not, right again once
    // looked at.
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

    // Pills: only when so.
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

    // Mic action: icon is what a tap does, and a tap does it.
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

    // Their picture behind all; no avatar over name.
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

    // Over: grid and action again.
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

    // Call fits cover at phone sizes. Every case: nothing past side margins or into action strip,
    // nothing over anything else, name on one or two lines, largest words.
    assert_eq!(value("phone-theme"), "ok", "{context}");
    let fit = |label: &str| {
        let layout = value(label);
        let pairs: std::collections::HashMap<String, String> = layout
            .split('|')
            .filter_map(|pair| pair.split_once('='))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        move |key: &str| {
            pairs
                .get(key)
                .cloned()
                .unwrap_or_else(|| format!("<no {key} in {layout}>"))
        }
    };
    let size = |label: &str, key: &str| -> i32 {
        fit(label)(key)
            .parse()
            .unwrap_or_else(|_| panic!("{label}: no {key}. {context}"))
    };
    for label in [
        "fit-ringing",
        "fit-calling",
        "fit-voice-bare",
        "fit-short-bare-mic",
        "fit-short-bare-camera",
        "fit-long-bare-both",
        "fit-short-bare-both",
        "fit-small-short-bare",
        "fit-small-long-bare-both",
        "fit-long-pictured-both",
        "fit-voice-pictured",
    ] {
        let got = fit(label);
        assert_eq!(
            got("strays"),
            "",
            "{label}: the call is drawn past the cover's side margins or into \
             the strip the home screen draws the action in. {context}"
        );
        assert_eq!(
            got("crowded"),
            "",
            "{label}: the call's pieces are drawn over one another. {context}"
        );
        let lines = size(label, "lines");
        assert!(
            (1..=2).contains(&lines),
            "{label}: the name takes {lines} lines. {context}"
        );
        let (name, time, note) = (
            size(label, "name"),
            size(label, "time"),
            size(label, "note"),
        );
        assert!(
            name > time && time >= note,
            "{label}: the name is not the largest words on the cover, or the \
             pills' are larger than the time's ({name}, {time}, {note}). {context}"
        );
        assert!(
            name < 40,
            "{label}: the name is drawn as large as a page's title ({name}). {context}"
        );
    }

    // Long name: two lines, cut short (one-line depends on test font). No picture: avatar
    // while room, else gone.
    assert_eq!(size("fit-long-bare-both", "lines"), 2, "{context}");
    assert_eq!(size("fit-long-pictured-both", "lines"), 2, "{context}");
    assert_eq!(
        fit("fit-voice-bare")("avatar"),
        "true",
        "a voice call without a picture has no avatar, room or not. {context}"
    );
    assert_eq!(fit("fit-short-bare-mic")("avatar"), "true", "{context}");
    assert_eq!(
        fit("fit-long-bare-both")("avatar"),
        "false",
        "the avatar is squeezed in over a long name and both pills. {context}"
    );
    assert_eq!(
        fit("fit-small-long-bare-both")("avatar"),
        "false",
        "the avatar is squeezed into the small cover. {context}"
    );
    for label in ["fit-long-pictured-both", "fit-voice-pictured"] {
        assert_eq!(
            fit(label)("picture"),
            "true",
            "{label}: their picture did not load. {context}"
        );
        assert_eq!(fit(label)("avatar"), "false", "{label}. {context}");
    }
}
