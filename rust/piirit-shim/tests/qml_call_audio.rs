//! Where a call's sound comes out (`CallAudio.qml`): the earpiece for a
//! voice call, the loudspeaker for a video call, the reader's switch over
//! both, and nothing at all left behind.
//!
//! The call holds a resource set of class `aliencall` with ohm's resource
//! manager while its sound is meant for the earpiece, and none for the
//! loudspeaker. What is checked is what the manager is sent, in
//! libresource's own message layout: the set registered, its streams
//! named by the app's id, acquired, and given back for the loudspeaker,
//! when the call is over, when the manager refuses any of it -- which
//! leaves the call on the loudspeaker for good -- and on the way out; and
//! that nothing is asked for without the app's id, which would leave the
//! call's own streams held back.
//!
//! And that it goes one message at a time, each once the last is
//! answered, as libresource's own clients send them: the manager answers
//! a register only after asking the bus who sent it, and refuses anything
//! else about the set that comes before then. On a phone, a set whose
//! messages all went out at once was refused and let go of every time.

// Qt harness: see qml_chat_list.rs.
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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: holder }
        property QtObject manager: null
        property QtObject routes: null
        // What the manager has been sent, kept as it is sent: the last of
        // it is sent as the component goes, and goes with it.
        property string log: ''
        function load(url, appId) {
            holder.setSource(url, { appId: appId })
            manager = null
            routes = null
            var data = holder.item ? holder.item.data : []
            for (var i = 0; i < data.length; i++) {
                if (data[i].objectName === 'resourceManager') { manager = data[i] }
                if (data[i].objectName === 'routes') { routes = data[i] }
            }
            if (manager === null || routes === null) { return 'load-failed' }
            // The manager answers every message at once, and takes it.
            manager.replies = [9, 0, 1, 0, 'OK']
            var watched = manager
            log = watched.called
            watched.calledChanged.connect(function () { log = watched.called })
            return 'ok'
        }
        function unload() { holder.source = ''; return 'ok' }
        function set(name, on) { holder.item[name] = on; return said() }
        function choose(loud) { holder.item.choose(loud); return said() }
        // The manager answers every message sent from now on with this
        // status.
        function answer(errcod) {
            manager.replies = [9, holder.item.held, 1, errcod, errcod === 0 ? 'OK' : 'Permission denied']
            return 'ok'
        }
        function fail() {
            manager.failure = ['org.freedesktop.DBus.Error.ServiceUnknown', 'no manager']
            return 'ok'
        }
        function heal() {
            manager.replies = [9, 0, 1, 0, 'OK']
            manager.failure = undefined
            manager.deferred = false
            return 'ok'
        }
        // The manager answers later, when told to: the message that is
        // out, or every message until none is.
        function defer() { manager.deferred = true; return 'ok' }
        function flush() { manager.answer(); return said() }
        function drain() {
            while (manager.held.length > 0) { manager.answer() }
            return said()
        }
        // Where the policy says the sound is: a sink and its route type.
        function route(name, type) {
            routes.replies = [name, type, 'microphone', 6]
            routes.rcAudioRouteChanged(name, type)
            var asked = routes.called
            routes.called = ''
            return asked.split(' ')[0]
        }
        // What was sent since the last look, a message a line: its method
        // and its arguments' values, `register 0 1 1 1 0 0 0 1234 aliencall 0`.
        function said() {
            var lines = log.trim().split('\n').filter(function (line) { return line.length > 0 })
            if (holder.item !== null) { manager.called = '' }
            log = ''
            return lines.map(function (line) {
                var space = line.indexOf(' ')
                var args = JSON.parse(line.substring(space + 1))
                return [line.substring(0, space)].concat(args.map(function (arg) { return arg.value })).join(' ')
            }).join(';')
        }
        function state() {
            var item = holder.item
            return item.held + '|' + item.speaker + '|' + item.elsewhere + '|' + item.switchable
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_call_holds_its_sound_and_gives_it_back() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    common::register_dbus_enum();

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

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
    macro_rules! set {
        ($name:expr, $on:expr) => {
            call!("set", QString::from($name), $on)
        };
    }

    let url = common::component_url("CallAudio.qml");

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    single_shot(Duration::from_millis(300), move || unsafe {
        let load = |app: &str| call!("load", QString::from(url.as_str()), QString::from(app));
        record!("load", load("1234"));
        record!("start", call!("said"));

        // One message at a time, with a manager that answers when told
        // to: the register alone, then each of the rest once the last is
        // answered.
        call!("defer");
        record!("first", set!("talking", true));
        record!("first-state", call!("state"));
        record!("second", call!("flush"));
        // The loudspeaker, tapped before the set is had: what was still
        // to be said about it goes unsaid, and it is given back once the
        // message out is answered.
        record!("switch-early", call!("choose", true));
        record!("third", call!("flush"));
        record!("fourth", call!("flush"));
        record!("drained", call!("flush"));
        // Back to the earpiece: a set of its own. Over with its register
        // out: given back once the answer is in.
        record!("quiet-out", call!("choose", false));
        record!("over-out", set!("talking", false));
        record!("over-out-release", call!("flush"));
        record!("over-out-unregister", call!("flush"));
        record!("over-out-done", call!("flush"));
        call!("heal");

        // A voice call: on the earpiece, with a set registered, its
        // streams named, acquired.
        record!("voice", set!("talking", true));
        record!("voice-state", call!("state"));
        // The loudspeaker gives the set back; the earpiece takes a new
        // one.
        record!("loud", call!("choose", true));
        record!("loud-state", call!("state"));
        record!("quiet", call!("choose", false));
        // Pictures come: the reader has chosen, so it stays.
        record!("pictures-chosen", set!("watching", true));
        set!("watching", false);
        // Over: given back.
        record!("over", set!("talking", false));
        record!("over-state", call!("state"));
        record!("over-switch", call!("choose", true));
        // A second voice call: a set of its own, back on the earpiece;
        // its pictures coming put it on the loudspeaker.
        record!("again", set!("talking", true));
        record!("pictures", set!("watching", true));
        set!("watching", false);
        record!("again-over", set!("talking", false));

        // A video call: the loudspeaker from the start, nothing held.
        set!("video", true);
        record!("video", set!("talking", true));
        record!("video-state", call!("state"));
        // Where the sound is: a headset dims the switch, the loudspeaker
        // does not; not looked at once the call is over.
        record!(
            "headset",
            call!("route", QString::from("headset"), 1 | 8 | 256 | (1 << 27))
        );
        record!("headset-state", call!("state"));
        record!(
            "speaker",
            call!("route", QString::from("speaker"), 1 | 4 | (1 << 27))
        );
        record!("speaker-state", call!("state"));
        // Built in and wired at once is still the phone's own, as ohm
        // reads its route types.
        record!(
            "built-in-wired",
            call!("route", QString::from("lineout"), 1 | 4 | 8 | (1 << 27))
        );
        record!("built-in-wired-state", call!("state"));
        // The manager refuses the earpiece: the rest of the set is not
        // sent, it is given back, the call is on the loudspeaker for
        // good, and the switch then does nothing.
        call!("defer");
        call!("answer", 13);
        record!("refused-sent", call!("choose", false));
        record!("refused", call!("drain"));
        record!("refused-state", call!("state"));
        call!("heal");
        record!("refused-after", call!("choose", false));
        record!("refused-after-state", call!("state"));
        set!("talking", false);
        record!("refused-over", call!("said"));
        record!(
            "over-route",
            call!("route", QString::from("headset"), 1 | 8)
        );

        // The policy still says a headset, which a call looks at as it
        // starts; and the refusal was the last call's.
        set!("video", false);
        record!("ok", set!("talking", true));
        record!("ok-state", call!("state"));
        set!("talking", false);
        call!("said");

        // No manager on the bus: let go of once that is known, before
        // anything more is sent about the set.
        call!("defer");
        call!("fail");
        record!("absent-sent", set!("talking", true));
        record!("absent", call!("drain"));
        record!("absent-state", call!("state"));
        set!("talking", false);
        call!("heal");
        call!("said");

        // Pictures already up as the call starts carrying sound: the
        // loudspeaker from the start.
        set!("watching", true);
        record!("watched", set!("talking", true));
        record!("watched-state", call!("state"));
        set!("talking", false);
        set!("watching", false);
        call!("said");

        // A refusal that comes late, for a call already over: the next
        // call's set is kept.
        call!("defer");
        call!("answer", 13);
        set!("talking", true);
        set!("talking", false);
        set!("talking", true);
        call!("answer", 0);
        call!("said");
        record!("late", call!("drain"));
        record!("late-state", call!("state"));
        call!("heal");
        set!("talking", false);
        call!("said");

        // Gone mid-call, with the register not answered yet: given back
        // on the way out all the same, without waiting for an answer
        // that would come to nobody.
        call!("defer");
        set!("talking", true);
        call!("said");
        record!("unload", call!("unload"));
        record!("left", call!("said"));

        // Without the app's id, nothing is asked for: the loudspeaker,
        // for good.
        load("");
        record!("no-id", set!("talking", true));
        record!("no-id-state", call!("state"));
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

    assert_eq!(value("load"), "ok", "the component did not load. {context}");
    assert_eq!(
        value("start"),
        "",
        "a fresh start sent something. {context}"
    );
    assert_eq!(
        value("first"),
        "register 0 1 1 1 0 0 0 1234 aliencall 0",
        "more than the register went out before the manager answered it. {context}"
    );
    assert_eq!(value("first-state"), "1|false|false|true", "{context}");
    assert_eq!(
        value("second"),
        "audio 7 1 2 aliencall 1234 media.name 0 *",
        "the set's streams were not named once it was registered. {context}"
    );
    assert_eq!(
        value("switch-early"),
        "",
        "the set was given back before the message out was answered. {context}"
    );
    assert_eq!(
        value("third"),
        "release 4 1 4",
        "the acquire meant for the earpiece still went out, or the set was not given back. {context}"
    );
    assert_eq!(value("fourth"), "unregister 1 1 5", "{context}");
    assert_eq!(value("drained"), "", "{context}");
    assert_eq!(
        value("quiet-out"),
        "register 0 2 6 1 0 0 0 1234 aliencall 0",
        "back on the earpiece, no set was asked for. {context}"
    );
    assert_eq!(
        value("over-out"),
        "",
        "the set was given back before the message out was answered. {context}"
    );
    assert_eq!(
        value("over-out-release"),
        "release 4 2 9",
        "what was meant for a call that is over still went out, or the set was not given back. {context}"
    );
    assert_eq!(
        value("over-out-unregister"),
        "unregister 1 2 10",
        "{context}"
    );
    assert_eq!(value("over-out-done"), "", "{context}");
    assert_eq!(
        value("voice"),
        "register 0 3 11 1 0 0 0 1234 aliencall 0;\
         audio 7 3 12 aliencall 1234 media.name 0 *;\
         acquire 3 3 13",
        "a voice call does not hold an aliencall set for the earpiece. {context}"
    );
    assert_eq!(value("voice-state"), "3|false|false|true", "{context}");
    assert_eq!(
        value("loud"),
        "release 4 3 14;unregister 1 3 15",
        "the loudspeaker switch did not give the set back. {context}"
    );
    assert_eq!(
        value("loud-state"),
        "0|true|false|true",
        "on the loudspeaker, a set is held or the switch is not there to use. {context}"
    );
    assert_eq!(
        value("quiet"),
        "register 0 4 16 1 0 0 0 1234 aliencall 0;\
         audio 7 4 17 aliencall 1234 media.name 0 *;\
         acquire 3 4 18",
        "switched back, the call did not take a set for the earpiece. {context}"
    );
    assert_eq!(
        value("pictures-chosen"),
        "",
        "pictures overrode the reader's choice. {context}"
    );
    assert_eq!(
        value("over"),
        "release 4 4 19;unregister 1 4 20",
        "a call that is over did not give its set back. {context}"
    );
    assert_eq!(value("over-state"), "0|false|false|false", "{context}");
    assert_eq!(
        value("over-switch"),
        "",
        "the switch sent something with no call. {context}"
    );
    assert_eq!(
        value("again"),
        "register 0 5 21 1 0 0 0 1234 aliencall 0;\
         audio 7 5 22 aliencall 1234 media.name 0 *;\
         acquire 3 5 23",
        "a second call does not get a set of its own on the earpiece. {context}"
    );
    assert_eq!(
        value("pictures"),
        "release 4 5 24;unregister 1 5 25",
        "a voice call whose pictures came on stayed on the earpiece. {context}"
    );
    assert_eq!(
        value("again-over"),
        "",
        "a set already given back was given back again. {context}"
    );
    assert_eq!(
        value("video"),
        "",
        "a video call on the loudspeaker asked for a set. {context}"
    );
    assert_eq!(
        value("video-state"),
        "0|true|false|true",
        "a video call does not start on the loudspeaker, with the switch there to use. {context}"
    );
    assert_eq!(value("headset"), "ActiveRoutes", "{context}");
    assert_eq!(
        value("headset-state"),
        "0|true|true|false",
        "a headset does not dim the switch. {context}"
    );
    assert_eq!(value("speaker"), "ActiveRoutes", "{context}");
    assert_eq!(value("speaker-state"), "0|true|false|true", "{context}");
    assert_eq!(value("built-in-wired"), "ActiveRoutes", "{context}");
    assert_eq!(
        value("built-in-wired-state"),
        "0|true|false|true",
        "a route built into the phone dimmed the switch. {context}"
    );
    assert_eq!(
        value("refused-sent"),
        "register 0 6 26 1 0 0 0 1234 aliencall 0",
        "{context}"
    );
    assert_eq!(
        value("refused"),
        "release 4 6 29;unregister 1 6 30",
        "a set the manager refused was not given back, or acquired anyway. {context}"
    );
    assert_eq!(
        value("refused-state"),
        "0|true|false|false",
        "refused, the call is not on the loudspeaker, or the switch is still there to use. {context}"
    );
    assert_eq!(
        value("refused-after"),
        "",
        "refused once, the switch asked again. {context}"
    );
    assert_eq!(
        value("refused-after-state"),
        "0|true|false|false",
        "{context}"
    );
    assert_eq!(
        value("refused-over"),
        "",
        "a set already given back was given back again. {context}"
    );
    assert_eq!(
        value("over-route"),
        "",
        "a route change after the call was looked at. {context}"
    );
    assert_eq!(
        value("ok"),
        "register 0 7 31 1 0 0 0 1234 aliencall 0;\
         audio 7 7 32 aliencall 1234 media.name 0 *;\
         acquire 3 7 33",
        "a refusal outlived its call. {context}"
    );
    assert_eq!(
        value("ok-state"),
        "7|false|true|false",
        "a set the manager took was given back, or the route was not looked at. {context}"
    );
    assert_eq!(
        value("absent-sent"),
        "register 0 8 36 1 0 0 0 1234 aliencall 0",
        "{context}"
    );
    assert_eq!(
        value("absent"),
        "release 4 8 39;unregister 1 8 40",
        "with no manager there, the set was not let go of at once. {context}"
    );
    assert_eq!(value("absent-state"), "0|true|true|false", "{context}");
    assert_eq!(
        value("watched"),
        "",
        "a call whose pictures were up as it started asked for the earpiece. {context}"
    );
    assert_eq!(value("watched-state"), "0|true|true|false", "{context}");
    assert_eq!(
        value("late"),
        "release 4 9 44;unregister 1 9 45;\
         register 0 10 46 1 0 0 0 1234 aliencall 0;\
         audio 7 10 47 aliencall 1234 media.name 0 *;\
         acquire 3 10 48",
        "a late refusal for a call that is over gave back the next one's set. {context}"
    );
    assert_eq!(value("late-state"), "10|false|true|false", "{context}");
    assert_eq!(value("unload"), "ok", "{context}");
    assert_eq!(
        value("left"),
        "release 4 11 54;unregister 1 11 55",
        "a set held on the way out was not given back. {context}"
    );
    assert_eq!(
        value("no-id"),
        "",
        "a set was asked for without the app's id. {context}"
    );
    assert_eq!(
        value("no-id-state"),
        "0|true|false|false",
        "without the app's id, the call is not on the loudspeaker for good. {context}"
    );
}
