//! Where a call's sound comes out (`CallAudio.qml`): the earpiece for a
//! voice call, the loudspeaker for a video call, the reader's switch over
//! both -- and the sound never moved while a microphone of the call's may
//! be open.
//!
//! On Sailfish, moving the sound while the browser engine's microphone is
//! open aborts PulseAudio, and a few of those in a row leave the phone
//! with no sound at all. So what is checked first is when things are
//! sent: the set had before the call's page may come up (`ready`);
//! nothing sent while a microphone may be open (`open`) -- not for the
//! switch, not for pictures, not for a refusal that comes late, not on
//! the way out; a switch that has the page shut the microphone
//! (`micWanted`) and moves the sound only once the watcher says PulseAudio
//! has no recording stream of the app's left (`micKnown`, `micOpen`), then
//! lets the microphone back; and the set given back once the call is over
//! the same way -- or, with no watcher to tell, a while after.
//!
//! And what is sent, in libresource's own message layout: a set of class
//! `aliencall` registered, its streams named by the app's id, acquired,
//! with video playback for the loudspeaker (the policy's own loudspeaker
//! for a call); updated for a switch; given back when the manager refuses
//! any of it, or is not there; and nothing asked for without the app's
//! id, which would leave the call's own streams held back. One message at
//! a time, each once the last is answered, as libresource's own clients
//! send them: the manager answers a register only after asking the bus who
//! sent it, and refuses anything else about the set that comes before.
//!
//! And while a call rings, a set of class `ringtone`, which other apps'
//! music gives way to: sound alone, naming no streams; given back once
//! the ringing stops, and for a call answered only after the call's own
//! set is had, so the music does not come back in between; not asked for
//! again for a ring the manager refused.

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
            manager = find('resourceManager')
            routes = find('routes')
            if (manager === null || routes === null) { return 'load-failed' }
            // The manager answers every message at once, and takes it.
            manager.replies = [9, 0, 1, 0, 'OK']
            var watched = manager
            log = watched.called
            watched.calledChanged.connect(function () { log = watched.called })
            return 'ok'
        }
        function find(name) {
            var data = holder.item ? holder.item.data : []
            for (var i = 0; i < data.length; i++) {
                if (data[i].objectName === name) { return data[i] }
            }
            return null
        }
        function unload() { holder.source = ''; return said() }
        function set(name, on) { holder.item[name] = on; return said() }
        function choose(loud) { holder.item.choose(loud); return said() }
        // A timer's time is up, as if it had run: stopped, so that it
        // does not come round again.
        function fire(name) {
            var timer = find(name)
            if (timer === null) { return 'missing:' + name }
            timer.stop()
            timer.triggered()
            return said()
        }
        function ringHeld() { return '' + holder.item.ringHeld }
        function running(name) {
            var timer = find(name)
            return timer === null ? 'missing:' + name : '' + timer.running
        }
        function intervals() {
            return ['settling', 'quieting', 'shutting', 'patience', 'stalled', 'ringOver'].map(function (name) {
                return find(name).interval
            }).join(',')
        }
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
        // The set held and whether it is the loudspeaker's; the switch;
        // whether the page may be up, a microphone may be open, the page
        // may have it; a switch under way; and whether the reader can
        // switch.
        function state() {
            var item = holder.item
            return [item.held, item.heldLoud ? 'loud' : 'ear', item.speaker ? 'speaker' : 'earpiece',
                    item.ready ? 'ready' : 'held-back', item.open ? 'open' : 'shut',
                    item.micWanted ? 'mic' : 'no-mic', item.switching ? 'switching' : 'still',
                    item.switchable ? 'switchable' : 'fixed'].join('|')
        }
    }
";

/// A whole set asked for: set `id`, its requests numbered from `first`,
/// for the loudspeaker or not.
fn asked(id: u32, first: u32, loud: bool) -> String {
    let all = if loud { 3 } else { 1 };
    format!(
        "register 0 {id} {first} {all} 0 0 0 1234 aliencall 0;\
         audio 7 {id} {} aliencall 1234 media.name 0 *;\
         acquire 3 {id} {}",
        first + 1,
        first + 2
    )
}

/// A ringing call's set asked for: set `id`, its requests numbered from
/// `first`. Sound alone, and no streams named.
fn rung(id: u32, first: u32) -> String {
    format!(
        "register 0 {id} {first} 1 0 0 0 1234 ringtone 0;acquire 3 {id} {}",
        first + 1
    )
}

/// A set turned the other way out: updated, and acquired again.
fn changed(id: u32, first: u32, loud: bool) -> String {
    let all = if loud { 3 } else { 1 };
    format!(
        "update 2 {id} {first} {all} 0 0 0 1234 aliencall 0;acquire 3 {id} {}",
        first + 1
    )
}

/// A set given back: set `id`, its requests numbered from `first`.
fn given_back(id: u32, first: u32) -> String {
    format!("release 4 {id} {first};unregister 1 {id} {}", first + 1)
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_call_moves_its_sound_only_with_the_microphone_shut() {
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
    macro_rules! fire {
        ($name:expr) => {
            call!("fire", QString::from($name))
        };
    }
    macro_rules! running {
        ($name:expr) => {
            call!("running", QString::from($name))
        };
    }

    let url = common::component_url("CallAudio.qml");

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    single_shot(Duration::from_millis(300), move || unsafe {
        let load = |app: &str| call!("load", QString::from(url.as_str()), QString::from(app));
        record!("load", load("1234"));
        record!("start", call!("said"));
        record!("intervals", call!("intervals"));
        // The watcher is connected, and no microphone is open.
        set!("micKnown", true);

        // A voice call, with a manager that answers when told to: the
        // register alone, then each of the rest once the last is
        // answered; the page held back all the while.
        call!("defer");
        record!("voice", set!("talking", true));
        record!("voice-state", call!("state"));
        record!("voice-patience", running!("patience"));
        record!("voice-audio", call!("flush"));
        record!("voice-acquire", call!("flush"));
        record!("voice-acquire-settling", running!("settling"));
        record!("voice-had", call!("flush"));
        record!("voice-had-state", call!("state"));
        record!("voice-had-settling", running!("settling"));
        // The policy's moment to move the sound: then the page comes up
        // and may open the microphone.
        record!("voice-in", fire!("settling"));
        record!("voice-in-state", call!("state"));
        record!("voice-in-patience", running!("patience"));
        // The page has yet to open it: that is not a microphone shut.
        record!("voice-in-quieting", running!("quieting"));
        call!("heal");
        record!("voice-mic", set!("micOpen", true));

        // A headset: the switch is dimmed; the loudspeaker does not dim
        // it.
        record!(
            "headset",
            call!("route", QString::from("headset"), 1 | 8 | 256 | (1 << 27))
        );
        record!("headset-state", call!("state"));
        record!("headset-tap", call!("choose", true));
        record!(
            "speaker-route",
            call!("route", QString::from("speaker"), 1 | 4 | (1 << 27))
        );
        record!("speaker-route-state", call!("state"));

        // The loudspeaker, tapped: nothing moves while the microphone is
        // open; the page is told to shut it, and the switch is busy.
        record!("loud-tap", call!("choose", true));
        record!("loud-tap-state", call!("state"));
        record!("loud-tap-stalled", running!("stalled"));
        record!("loud-tap-again", call!("choose", false));
        record!("loud-tap-again-state", call!("state"));
        // Nothing that keeps time moves the sound or lets the microphone
        // back early.
        fire!("settling");
        fire!("shutting");
        fire!("patience");
        record!("loud-early", fire!("quieting"));
        record!("loud-early-state", call!("state"));
        // Shut for a moment, then not: not shut at all.
        set!("micOpen", false);
        record!("loud-flicker", running!("quieting"));
        set!("micOpen", true);
        record!("loud-flicker-quieting", running!("quieting"));
        record!("loud-flicker-state", call!("state"));
        // PulseAudio says it has shut, and still does a moment later: the
        // set turned for the loudspeaker.
        record!("loud-shut", set!("micOpen", false));
        record!("loud-shut-quieting", running!("quieting"));
        record!("loud-moved", fire!("quieting"));
        record!("loud-moved-state", call!("state"));
        // The policy's moment, and the microphone back.
        record!("loud-in", fire!("settling"));
        record!("loud-in-state", call!("state"));
        record!("loud-back", set!("micOpen", true));
        record!("loud-back-state", call!("state"));
        record!("loud-back-stalled", running!("stalled"));

        // Over: nothing until the microphone has shut, then given back.
        record!("over", set!("talking", false));
        record!("over-state", call!("state"));
        record!("over-quieting", running!("quieting"));
        set!("micOpen", false);
        record!("over-shut", fire!("quieting"));
        record!("over-shut-state", call!("state"));

        // A video call: the loudspeaker from the start, video playback in
        // the set; switched to the earpiece.
        set!("video", true);
        record!("video", set!("talking", true));
        record!("video-state", call!("state"));
        fire!("settling");
        set!("micOpen", true);
        call!("choose", false);
        set!("micOpen", false);
        record!("video-ear", fire!("quieting"));
        fire!("settling");
        set!("micOpen", true);
        // Over, with the watcher gone: given back a while after.
        set!("talking", false);
        record!("blind-over", set!("micKnown", false));
        // A watcher that cannot tell says no microphone is open.
        set!("micOpen", false);
        record!("blind-over-shutting", running!("shutting"));
        record!("blind-over-quieting", running!("quieting"));
        record!("blind-shut", fire!("shutting"));
        set!("micOpen", false);
        set!("micKnown", true);
        set!("video", false);

        // No watcher, a microphone open: the switch is dimmed, and
        // pictures do not move the sound either.
        record!("blind", set!("talking", true));
        fire!("settling");
        set!("micOpen", true);
        set!("micKnown", false);
        record!("blind-tap", call!("choose", true));
        record!("blind-state", call!("state"));
        record!("blind-pictures", set!("watching", true));
        record!("blind-pictures-state", call!("state"));
        set!("watching", false);
        set!("micKnown", true);

        // A microphone that does not shut: given up on, back on, the
        // sound where it was.
        call!("choose", true);
        record!("stuck", fire!("stalled"));
        record!("stuck-state", call!("state"));
        // A policy that does not answer the switch: given up on, the
        // microphone back, and the rest not sent while it is open.
        call!("defer");
        call!("choose", true);
        set!("micOpen", false);
        record!("silent-switch", fire!("quieting"));
        record!("silent-switch-gave-up", fire!("stalled"));
        record!("silent-switch-state", call!("state"));
        record!("silent-switch-late", call!("flush"));
        set!("micOpen", true);
        set!("talking", false);
        set!("micOpen", false);
        record!("silent-switch-over", fire!("quieting"));
        record!("silent-switch-rest", call!("drain"));
        call!("heal");

        // Pictures coming put a voice call on the loudspeaker, unless the
        // reader has switched it.
        record!("pictures-call", set!("talking", true));
        fire!("settling");
        set!("micOpen", true);
        record!("pictures", set!("watching", true));
        record!("pictures-state", call!("state"));
        set!("micOpen", false);
        record!("pictures-moved", fire!("quieting"));
        fire!("settling");
        set!("micOpen", true);
        set!("watching", false);
        call!("choose", false);
        set!("micOpen", false);
        record!("chosen-moved", fire!("quieting"));
        fire!("settling");
        set!("micOpen", true);
        record!("chosen-pictures", set!("watching", true));
        record!("chosen-pictures-state", call!("state"));
        set!("watching", false);
        set!("talking", false);
        set!("micOpen", false);
        record!("pictures-over", fire!("quieting"));

        // A call that starts while the last one's microphone is still
        // open waits for it to shut; one like it keeps the set.
        record!("first-call", set!("talking", true));
        fire!("settling");
        set!("micOpen", true);
        set!("talking", false);
        record!("next-call", set!("talking", true));
        record!("next-call-state", call!("state"));
        // Tapped meanwhile: nothing is open of this call's to shut.
        call!("choose", true);
        record!("next-call-tap", call!("state"));
        call!("choose", false);
        set!("micOpen", false);
        record!("next-call-shut", fire!("quieting"));
        record!("next-call-settling", running!("settling"));
        fire!("settling");
        record!("next-call-state-in", call!("state"));
        set!("micOpen", true);
        set!("talking", false);
        set!("micOpen", false);
        record!("next-call-over", fire!("quieting"));

        // The manager refuses the earpiece: the rest of the set is not
        // sent, it is given back, and the call is on the loudspeaker for
        // good, with the switch dimmed.
        call!("defer");
        call!("answer", 13);
        record!("refused-sent", set!("talking", true));
        record!("refused", call!("drain"));
        record!("refused-state", call!("state"));
        call!("heal");
        fire!("settling");
        set!("micOpen", true);
        record!("refused-tap", call!("choose", false));
        set!("talking", false);
        set!("micOpen", false);
        record!("refused-over", fire!("quieting"));
        // The refusal was that call's. Over before the microphone ever
        // opened: given back once the moment is up.
        record!("after-refusal", set!("talking", true));
        fire!("settling");
        set!("talking", false);
        record!("after-refusal-quieting", running!("quieting"));
        record!("after-refusal-over", fire!("quieting"));

        // Refused mid-switch: given back with the microphone shut, and
        // the microphone back on the loudspeaker.
        record!("mid-call", set!("talking", true));
        fire!("settling");
        set!("micOpen", true);
        call!("choose", true);
        set!("micOpen", false);
        call!("defer");
        call!("answer", 13);
        record!("mid-update", fire!("quieting"));
        record!("mid-refused", call!("drain"));
        call!("heal");
        fire!("settling");
        set!("micOpen", true);
        record!("mid-refused-state", call!("state"));
        set!("talking", false);
        set!("micOpen", false);
        record!("mid-over", fire!("quieting"));

        // No manager on the bus: let go of once that is known.
        call!("defer");
        call!("fail");
        record!("absent-sent", set!("talking", true));
        record!("absent", call!("drain"));
        record!("absent-state", call!("state"));
        fire!("settling");
        set!("talking", false);
        record!("absent-over", fire!("quieting"));
        call!("heal");

        // A manager that does not answer the call's set at all: the page
        // comes up without it, and the rest waits for the call to be
        // over and its microphone shut.
        call!("defer");
        record!("silent-sent", set!("talking", true));
        record!("silent-in", fire!("patience"));
        record!("silent-state", call!("state"));
        record!("silent-late", call!("flush"));
        set!("talking", false);
        record!("silent-over", fire!("quieting"));
        record!("silent-rest", call!("drain"));
        call!("heal");

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
        fire!("settling");
        set!("talking", false);
        record!("late-over", fire!("quieting"));

        // A call ringing: the ring's set, given back a moment after the
        // ringing stops -- declined, missed, or taken elsewhere.
        record!("ring", set!("ringing", true));
        record!("ring-state", call!("state"));
        record!("ring-held", call!("ringHeld"));
        record!("ring-over", set!("ringing", false));
        record!("ring-over-waits", running!("ringOver"));
        record!("ring-gone", fire!("ringOver"));
        record!("ring-gone-held", call!("ringHeld"));

        // Answered, with a manager that answers when told to: no longer
        // ringing, then carrying sound, as an answer changes the call.
        // The call's set is had before the ring's goes, one message at a
        // time, and the page's moment comes only after both.
        call!("defer");
        record!("answer-ring", set!("ringing", true));
        record!("answer-ring-acquire", call!("flush"));
        record!("answer-ring-had", call!("flush"));
        set!("ringing", false);
        // Looked at in between, the ring's set still stays.
        record!("answered-between", set!("speaker", true));
        record!("answered", set!("talking", true));
        record!("answered-turn", fire!("ringOver"));
        record!("answered-rest", call!("drain"));
        record!("answered-state", call!("state"));
        record!("answered-held", call!("ringHeld"));
        call!("heal");
        record!("answered-in", fire!("settling"));
        set!("micOpen", true);
        set!("talking", false);
        set!("micOpen", false);
        record!("answered-over", fire!("quieting"));

        // The other way round: carrying sound, then no longer ringing.
        record!("again-ring", set!("ringing", true));
        record!("again-answered", set!("talking", true));
        record!("again-unrung", set!("ringing", false));
        record!("again-ring-gone", fire!("ringOver"));
        record!("again-settling", running!("settling"));
        fire!("settling");
        set!("micOpen", true);
        set!("talking", false);
        set!("micOpen", false);
        record!("again-over", fire!("quieting"));

        // A ring the manager refuses: given back, and not asked for
        // again while that call rings; the next call's ring asks anew.
        call!("defer");
        call!("answer", 13);
        record!("ring-refused-sent", set!("ringing", true));
        record!("ring-refused", call!("drain"));
        record!("ring-refused-held", call!("ringHeld"));
        call!("heal");
        record!("ring-refused-again", set!("speaker", true));
        set!("speaker", false);
        set!("ringing", false);
        record!("ring-refused-over", fire!("ringOver"));
        record!("next-ring", set!("ringing", true));
        set!("ringing", false);
        record!("next-ring-over", fire!("ringOver"));

        // Gone mid-call, the microphone open: nothing sent on the way out.
        set!("talking", true);
        fire!("settling");
        set!("micOpen", true);
        call!("said");
        record!("left-open", call!("unload"));

        // Gone with the register not answered yet, before the page could
        // come up: given back on the way out, without waiting for an
        // answer that would come to nobody.
        load("1234");
        set!("micKnown", true);
        call!("defer");
        set!("talking", true);
        call!("said");
        record!("left", call!("unload"));

        // Gone while a call rings: the ring's set given back on the way
        // out.
        load("1234");
        set!("ringing", true);
        call!("said");
        record!("left-ringing", call!("unload"));

        // Without the app's id, nothing is asked for: the loudspeaker,
        // for good, and the page need not wait for the policy.
        load("");
        record!("no-id-ring", set!("ringing", true));
        set!("ringing", false);
        record!("no-id", set!("talking", true));
        record!("no-id-settling", running!("settling"));
        fire!("settling");
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

    let expected: Vec<(&str, String, &str)> = vec![
        ("load", "ok".into(), "the component did not load"),
        ("start", String::new(), "a fresh start sent something"),
        (
            "intervals",
            "500,300,3000,5000,5000,0".into(),
            "a wait changed: the sound's moment, the microphone staying shut, a microphone \
             taken as shut without the watcher, the policy at the start, a switch, the ring's \
             end looked at once the call's state has settled",
        ),
        (
            "voice",
            "register 0 1 1 1 0 0 0 1234 aliencall 0".into(),
            "more than the register went out before the manager answered it",
        ),
        (
            "voice-state",
            "1|ear|earpiece|held-back|shut|no-mic|still|switchable".into(),
            "a voice call is not meant for the earpiece, or its page is let up before its set is had",
        ),
        ("voice-patience", "true".into(), "the call does not keep time on the policy"),
        (
            "voice-audio",
            "audio 7 1 2 aliencall 1234 media.name 0 *".into(),
            "the set's streams were not named once it was registered",
        ),
        ("voice-acquire", "acquire 3 1 3".into(), "the set was not acquired"),
        ("voice-acquire-settling", "false".into(), "the page's moment came with the acquire still out"),
        ("voice-had", String::new(), "something went out after the acquire"),
        (
            "voice-had-state",
            "1|ear|earpiece|held-back|shut|no-mic|still|switchable".into(),
            "the page is let up the moment the set is had, before the policy moves the sound",
        ),
        ("voice-had-settling", "true".into(), "the page is not given its moment once the set is had"),
        ("voice-in", String::new(), "something went out as the page was let up"),
        (
            "voice-in-state",
            "1|ear|earpiece|ready|open|mic|still|switchable".into(),
            "the page was not let up, or not let have the microphone",
        ),
        ("voice-in-patience", "false".into(), "the call still waits on the policy once its page is up"),
        (
            "voice-in-quieting",
            "false".into(),
            "a microphone the page has yet to open is taken as one shut",
        ),
        ("voice-mic", String::new(), "the microphone opening sent something"),
        ("headset", "ActiveRoutes".into(), "a route change was not looked at"),
        (
            "headset-state",
            "1|ear|earpiece|ready|open|mic|still|fixed".into(),
            "a headset does not dim the switch",
        ),
        ("headset-tap", String::new(), "a dimmed switch did something"),
        ("speaker-route", "ActiveRoutes".into(), "{context}"),
        ("speaker-route-state", "1|ear|earpiece|ready|open|mic|still|switchable".into(), "{context}"),
        (
            "loud-tap",
            String::new(),
            "the sound was moved with the microphone open",
        ),
        (
            "loud-tap-state",
            "1|ear|speaker|ready|open|no-mic|switching|fixed".into(),
            "the page was not told to shut the microphone, or the switch is not busy",
        ),
        ("loud-tap-stalled", "true".into(), "a switch does not keep time"),
        ("loud-tap-again", String::new(), "a busy switch did something"),
        (
            "loud-tap-again-state",
            "1|ear|speaker|ready|open|no-mic|switching|fixed".into(),
            "a busy switch was turned back",
        ),
        ("loud-early", String::new(), "a timer moved the sound with the microphone open"),
        (
            "loud-early-state",
            "1|ear|speaker|ready|open|no-mic|switching|fixed".into(),
            "a timer let the microphone back, or took it as shut, mid-switch",
        ),
        ("loud-flicker", "true".into(), "{context}"),
        (
            "loud-flicker-quieting",
            "false".into(),
            "a microphone open again is still waited on as shut",
        ),
        (
            "loud-flicker-state",
            "1|ear|speaker|ready|open|no-mic|switching|fixed".into(),
            "a microphone open again, before the sound moved, ended the switch",
        ),
        ("loud-shut", String::new(), "the sound moved the moment the watcher spoke, not a moment after"),
        ("loud-shut-quieting", "true".into(), "the microphone shutting is not waited on"),
        (
            "loud-moved",
            changed(1, 4, true),
            "with the microphone shut, the set was not turned for the loudspeaker",
        ),
        (
            "loud-moved-state",
            "1|loud|speaker|ready|shut|no-mic|switching|fixed".into(),
            "{context}",
        ),
        ("loud-in", String::new(), "{context}"),
        (
            "loud-in-state",
            "1|loud|speaker|ready|open|mic|switching|fixed".into(),
            "the microphone was not let back once the sound had moved",
        ),
        ("loud-back", String::new(), "{context}"),
        (
            "loud-back-state",
            "1|loud|speaker|ready|open|mic|still|switchable".into(),
            "the switch is still busy with the microphone back",
        ),
        ("loud-back-stalled", "false".into(), "a finished switch still keeps time"),
        (
            "over",
            String::new(),
            "the set was given back with the call's microphone still open",
        ),
        (
            "over-state",
            "1|loud|speaker|held-back|open|no-mic|still|fixed".into(),
            "a call that is over still lets its page up, or forgot its microphone is open",
        ),
        ("over-quieting", "false".into(), "a microphone still open is taken as shut"),
        ("over-shut", given_back(1, 6), "once the microphone shut, the set was not given back"),
        (
            "over-shut-state",
            "0|loud|speaker|held-back|shut|no-mic|still|fixed".into(),
            "{context}",
        ),
        (
            "video",
            asked(2, 8, true),
            "a video call does not ask for video playback with its sound: the loudspeaker",
        ),
        (
            "video-state",
            "2|loud|speaker|held-back|shut|no-mic|still|switchable".into(),
            "{context}",
        ),
        ("video-ear", changed(2, 11, false), "a video call was not switched to the earpiece"),
        ("blind-over", String::new(), "{context}"),
        ("blind-over-shutting", "true".into(), "without the watcher, nothing waits for the microphone"),
        ("blind-over-quieting", "false".into(), "{context}"),
        (
            "blind-shut",
            given_back(2, 13),
            "without the watcher, the set was not given back a while after the call",
        ),
        ("blind", asked(3, 15, false), "{context}"),
        ("blind-tap", String::new(), "without the watcher, the switch moved the sound"),
        (
            "blind-state",
            "3|ear|earpiece|ready|open|mic|still|fixed".into(),
            "without the watcher, the switch is not dimmed",
        ),
        ("blind-pictures", String::new(), "without the watcher, pictures moved the sound"),
        (
            "blind-pictures-state",
            "3|ear|earpiece|ready|open|mic|still|fixed".into(),
            "without the watcher, pictures left the switch saying the loudspeaker",
        ),
        ("stuck", String::new(), "{context}"),
        (
            "stuck-state",
            "3|ear|earpiece|ready|open|mic|still|switchable".into(),
            "a switch given up on did not put the microphone back, or the switch as the sound is",
        ),
        ("silent-switch", "update 2 3 18 3 0 0 0 1234 aliencall 0".into(), "{context}"),
        ("silent-switch-gave-up", String::new(), "{context}"),
        (
            "silent-switch-state",
            "3|loud|speaker|ready|open|mic|still|switchable".into(),
            "a switch the policy did not answer left the microphone shut",
        ),
        (
            "silent-switch-late",
            String::new(),
            "the rest of the switch went out with the microphone back",
        ),
        ("silent-switch-over", "release 4 3 20".into(), "{context}"),
        ("silent-switch-rest", "unregister 1 3 21".into(), "{context}"),
        ("pictures-call", asked(4, 22, false), "{context}"),
        ("pictures", String::new(), "pictures moved the sound with the microphone open"),
        (
            "pictures-state",
            "4|ear|speaker|ready|open|no-mic|switching|fixed".into(),
            "pictures did not start a switch to the loudspeaker",
        ),
        ("pictures-moved", changed(4, 25, true), "{context}"),
        ("chosen-moved", changed(4, 27, false), "{context}"),
        ("chosen-pictures", String::new(), "pictures overrode the reader's choice"),
        (
            "chosen-pictures-state",
            "4|ear|earpiece|ready|open|mic|still|switchable".into(),
            "pictures overrode the reader's choice",
        ),
        ("pictures-over", given_back(4, 29), "{context}"),
        ("first-call", asked(5, 31, false), "{context}"),
        (
            "next-call",
            String::new(),
            "a call asked the policy for something with the last call's microphone open",
        ),
        (
            "next-call-state",
            "5|ear|earpiece|held-back|open|no-mic|still|switchable".into(),
            "a new call let its page up with the last call's microphone open",
        ),
        (
            "next-call-tap",
            "5|ear|speaker|held-back|open|no-mic|still|switchable".into(),
            "a call not yet let up waits on a switch of its own",
        ),
        ("next-call-shut", String::new(), "a set like the one held was given back and asked for again"),
        ("next-call-settling", "true".into(), "the new call's page is not given its moment"),
        (
            "next-call-state-in",
            "5|ear|earpiece|ready|open|mic|still|switchable".into(),
            "{context}",
        ),
        ("next-call-over", given_back(5, 34), "{context}"),
        ("refused-sent", "register 0 6 36 1 0 0 0 1234 aliencall 0".into(), "{context}"),
        (
            "refused",
            given_back(6, 39),
            "a set the manager refused was not given back, or acquired anyway",
        ),
        (
            "refused-state",
            "0|ear|speaker|held-back|shut|no-mic|still|fixed".into(),
            "refused, the call is not on the loudspeaker, or the switch is not dimmed",
        ),
        ("refused-tap", String::new(), "refused once, the switch asked again"),
        ("refused-over", String::new(), "a set already given back was given back again"),
        ("after-refusal", asked(7, 41, false), "a refusal outlived its call"),
        (
            "after-refusal-quieting",
            "true".into(),
            "a page let up and gone is not waited out before the set goes",
        ),
        ("after-refusal-over", given_back(7, 44), "{context}"),
        ("mid-call", asked(8, 46, false), "{context}"),
        ("mid-update", "update 2 8 49 3 0 0 0 1234 aliencall 0".into(), "{context}"),
        (
            "mid-refused",
            given_back(8, 51),
            "an update the manager refused did not give the set back",
        ),
        (
            "mid-refused-state",
            "0|loud|speaker|ready|open|mic|still|fixed".into(),
            "refused mid-switch, the microphone did not come back, or the switch is not dimmed",
        ),
        ("mid-over", String::new(), "{context}"),
        ("absent-sent", "register 0 9 53 1 0 0 0 1234 aliencall 0".into(), "{context}"),
        (
            "absent",
            given_back(9, 56),
            "with no manager there, the set was not let go of at once",
        ),
        (
            "absent-state",
            "0|ear|speaker|held-back|shut|no-mic|still|fixed".into(),
            "{context}",
        ),
        ("absent-over", String::new(), "{context}"),
        ("silent-sent", "register 0 10 58 1 0 0 0 1234 aliencall 0".into(), "{context}"),
        ("silent-in", String::new(), "{context}"),
        (
            "silent-state",
            "10|ear|earpiece|ready|open|mic|still|switchable".into(),
            "a manager that does not answer keeps the call's page back for good",
        ),
        (
            "silent-late",
            String::new(),
            "the rest of the set went out with the page up",
        ),
        (
            "silent-over",
            "release 4 10 61".into(),
            "what was not sent was sent after all, or the set not given back",
        ),
        ("silent-rest", "unregister 1 10 62".into(), "{context}"),
        (
            "late",
            format!("{};{}", given_back(11, 66), asked(12, 68, false)),
            "a late refusal for a call that is over gave back the next one's set",
        ),
        (
            "late-state",
            "12|ear|earpiece|held-back|shut|no-mic|still|switchable".into(),
            "{context}",
        ),
        ("late-over", given_back(12, 71), "{context}"),
        (
            "ring",
            rung(13, 73),
            "a ringing call did not ask for the ring's set, or asked for more than sound",
        ),
        (
            "ring-state",
            "0|ear|earpiece|held-back|shut|no-mic|still|fixed".into(),
            "a ringing call let the page up, or asked for the call's own set",
        ),
        ("ring-held", "13".into(), "{context}"),
        ("ring-over", String::new(), "the ring's set went before the call's state had settled"),
        ("ring-over-waits", "true".into(), "{context}"),
        ("ring-gone", given_back(13, 75), "the ring's set was not given back once it stopped ringing"),
        ("ring-gone-held", "0".into(), "{context}"),
        (
            "answer-ring",
            "register 0 14 77 1 0 0 0 1234 ringtone 0".into(),
            "more than the ring's register went out before it was answered",
        ),
        ("answer-ring-acquire", "acquire 3 14 78".into(), "{context}"),
        ("answer-ring-had", String::new(), "{context}"),
        (
            "answered-between",
            String::new(),
            "the ring's set went before the call's state had settled",
        ),
        (
            "answered",
            "register 0 15 79 1 0 0 0 1234 aliencall 0".into(),
            "an answered call did not ask for its own set, or the ring's went first",
        ),
        ("answered-turn", String::new(), "something went out with a message still unanswered"),
        (
            "answered-rest",
            format!(
                "audio 7 15 80 aliencall 1234 media.name 0 *;acquire 3 15 81;{}",
                given_back(14, 82)
            ),
            "the ring's set went before the call's own was had: the music would come back",
        ),
        (
            "answered-state",
            "15|ear|earpiece|held-back|shut|no-mic|still|switchable".into(),
            "{context}",
        ),
        ("answered-held", "0".into(), "{context}"),
        ("answered-in", String::new(), "{context}"),
        ("answered-over", given_back(15, 84), "{context}"),
        ("again-ring", rung(16, 86), "{context}"),
        ("again-answered", asked(17, 88, false), "{context}"),
        ("again-unrung", String::new(), "{context}"),
        ("again-ring-gone", given_back(16, 91), "{context}"),
        ("again-settling", "true".into(), "{context}"),
        ("again-over", given_back(17, 93), "{context}"),
        (
            "ring-refused-sent",
            "register 0 18 95 1 0 0 0 1234 ringtone 0".into(),
            "{context}",
        ),
        (
            "ring-refused",
            given_back(18, 97),
            "a ring's set the manager refused was not given back, or acquired anyway",
        ),
        ("ring-refused-held", "0".into(), "{context}"),
        (
            "ring-refused-again",
            String::new(),
            "a ring's set refused was asked for again while the call rang",
        ),
        ("ring-refused-over", String::new(), "a ring's set already given back was given back again"),
        ("next-ring", rung(19, 99), "a refusal outlived the ring it was for"),
        ("next-ring-over", given_back(19, 101), "{context}"),
        (
            "left-open",
            String::new(),
            "the set was given back on the way out with the microphone open",
        ),
        (
            "left",
            given_back(1, 4),
            "a set held on the way out, the page not yet up, was not given back",
        ),
        (
            "left-ringing",
            given_back(1, 3),
            "the ring's set was not given back on the way out",
        ),
        ("no-id-ring", String::new(), "a ring's set was asked for without the app's id"),
        ("no-id", String::new(), "a set was asked for without the app's id"),
        ("no-id-settling", "true".into(), "{context}"),
        (
            "no-id-state",
            "0|ear|speaker|ready|open|mic|still|fixed".into(),
            "without the app's id, the call is not on the loudspeaker for good",
        ),
    ];
    for (label, want, why) in expected {
        assert_eq!(value(label), want, "{label}: {why}. {context}");
    }
}
