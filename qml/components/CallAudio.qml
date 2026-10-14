import QtQuick 2.0
import Nemo.DBus 2.0

/*
 * Where a call's sound comes out: the earpiece or the loudspeaker, or
 * whatever is plugged in or paired.
 *
 * - A voice call is on the earpiece, as a phone call is, and a video call
 *   on the loudspeaker, as the reference clients have it: a video call is
 *   held in front of the face. A voice call whose pictures come on goes
 *   to the loudspeaker too, unless the reader has switched this call.
 * - The reader switches between the two (the call page's loudspeaker
 *   switch), and that holds for the rest of the call. The microphone
 *   goes quiet for a moment while the sound moves, and the switch says
 *   it is busy until it is back.
 * - A headset, headphones or a Bluetooth device takes the sound by
 *   itself, and the switch has nothing to move it from.
 *
 * Why the microphone goes quiet: on Sailfish, moving the sound while the
 * call's microphone is open brings the sound server down. The browser
 * engine opens the microphone pinned to its device (cubeb's PulseAudio
 * backend marks a stream opened on a named device as not to be moved),
 * and the policy's PulseAudio module moves every call stream to the new
 * route, asserting that each one went: the one that cannot aborts
 * PulseAudio (policy-group.c, start_move_group). Restarted, it has
 * forgotten which streams were the call's and holds them back as any
 * other app's; a few of those in a row and systemd stops restarting it,
 * and the phone has no sound at all until someone who knows how starts
 * it again. A headset plugged in mid-call does the same, and nothing
 * here can stop that. What the app does is never move the sound itself
 * while a microphone of the call's may be open (`open`):
 *
 * - the call's set is had before the microphone first opens: the call's
 *   page does not come up until then (`ready`, CallView.allowed);
 * - to switch, the page is told to stop the microphone (`micWanted`),
 *   and the sound is moved once PulseAudio itself has no recording
 *   stream of the app's left (MicWatch: `micKnown`, `micOpen`); then the
 *   page opens it again;
 * - once the call is over, the set is given back the same way: once the
 *   microphone has shut.
 *
 * With the watcher unable to tell, a microphone told to stop is taken
 * as shut a few seconds later, and the reader cannot switch while one
 * may be open.
 *
 * The set is of class `aliencall`, with ohm's resource manager on the
 * system bus: the class Sailfish has for calls that are not phone calls,
 * which the policy routes as a call -- the earpiece, or a headset or a
 * paired device before it -- or, with video playback in the set as
 * well, to the loudspeaker. It names the app's streams by the id
 * libpulse gives them (Call.audio_id), so the call's sound joins the
 * call group rather than being held back as any other app's is while a
 * call is on; other apps' music pauses for it.
 *
 * Nothing is written that outlives the app: ohm drops a client's sets
 * the moment its bus connection goes, so a crash or a kill puts the
 * sound back where it was without anybody doing anything. A set that
 * cannot be had -- refused, or no manager to answer, or no id to name
 * the streams by -- is let go of, and the call's sound goes where any
 * app's goes: the loudspeaker, for good.
 *
 * While a call rings, a second set is held, of class `ringtone`: the
 * class the policy has for a phone ringing, which other apps' music
 * gives way to, as it does for a phone call. It names no streams (ngfd
 * plays the ringtone, and the policy knows its streams for a ringtone's
 * by themselves) and asks for nothing but sound, so it moves nothing of
 * the app's. It goes when the ringing stops -- for a call answered, only
 * after the call's own set is had, so that the music held off for the
 * ring stays off for the call.
 * docs/CALLS.md, "Earpiece and loudspeaker".
 */
Item {
    id: audio

    /// The call carries sound: placed or answered, and not over.
    property bool talking: false

    /// A call is ringing here.
    property bool ringing: false

    /// The call was placed or answered with pictures.
    property bool video: false

    /// There are pictures on the call's screen.
    property bool watching: false

    /// The id the policy knows this app's streams by (Call.audio_id);
    /// with none, nothing is asked for.
    property string appId: ""

    /// The microphone watcher can tell (MicWatch.known), and what it
    /// tells: a recording stream of the app's is open in PulseAudio
    /// (MicWatch.open).
    property bool micKnown: false
    property bool micOpen: false

    /// The sound is meant for the loudspeaker rather than the earpiece.
    property bool speaker: false

    /// The reader switched it, this call: the pictures no longer decide.
    property bool chosen: false

    /// The sound is on something plugged in or paired, as the policy
    /// says: the loudspeaker switch has nothing to move it from.
    property bool elsewhere: false

    /// The policy would not take this call's set, or there is no id to
    /// name its streams by: the call is on the loudspeaker for good.
    property bool denied: false

    /// The call's page may come up (CallView.allowed): the call's sound
    /// has its way out. Until the call is over.
    property bool ready: false

    /// The page may hold the microphone. Not while the sound moves, and
    /// not before it has its way out.
    property bool micWanted: false

    /// A microphone of the call's may be open: from when the page is let
    /// have it until it has surely shut. Nothing is sent meanwhile.
    property bool open: false

    /// The reader's switch is being carried out: from the tap until the
    /// microphone is back.
    property bool switching: false

    /// The reader can switch: there is a call, its sound is on the phone
    /// itself, under the call's own set, and the microphone can be seen
    /// to shut.
    readonly property bool switchable: talking && !denied && !elsewhere && !switching
                                       && (micKnown || !open)

    /// The resource set held, by its number; 0 for none.
    property int held: 0

    /// Whether the set held asks for video playback too: the
    /// loudspeaker.
    property bool heldLoud: false

    /// The ringing call's set, by its number; 0 for none.
    property int ringHeld: 0

    /// The policy would not take the ringing call's set: not asked for
    /// again while this call rings.
    property bool ringRefused: false

    /// The last set's number. Each set gets a new one: the manager knows
    /// a set by the client and its number.
    property int count: 0

    /// The last request's number, which the manager answers by.
    property int reqno: 0

    /// What is still to be sent, oldest first: `{ id, method, args }`.
    property var queue: []

    /// A message is out, and its answer not in yet.
    property bool waiting: false

    // From libresource (res-msg.h, res-types.h): message types, and the
    // resources a set holds.
    readonly property int msgRegister: 0
    readonly property int msgUnregister: 1
    readonly property int msgUpdate: 2
    readonly property int msgAcquire: 3
    readonly property int msgRelease: 4
    readonly property int msgAudio: 7
    readonly property int audioPlayback: 1
    readonly property int videoPlayback: 2

    // Route types, from ohm's route interface (ohm-ext/route.h).
    readonly property int routeBuiltin: 4
    readonly property int routeWired: 8
    readonly property int routeWireless: 16

    // Once the set is had, a moment for the policy to move the sound
    // before the microphone may open: ohm answers a request, and only
    // then tells PulseAudio where the sound goes.
    Timer {
        id: settling
        objectName: "settling"
        interval: 500
        onTriggered: {
            if (audio.talking && !audio.open && !audio.waiting && audio.queue.length === 0) {
                audio.letIn()
            }
        }
    }

    // The watcher says the microphone has shut: and still does a moment
    // later, so a stream that was only just opening is seen too.
    Timer {
        id: quieting
        objectName: "quieting"
        interval: 300
        onTriggered: {
            if (audio.quiet()) {
                audio.open = false
                audio.settle()
            }
        }
    }

    // With nothing to tell whether the microphone has shut, how long
    // after it was told to before it surely has. The engine stops it off
    // the event loop as it tears the page down, or as it is told.
    Timer {
        id: shutting
        objectName: "shutting"
        interval: 3000
        onTriggered: {
            if (audio.told() && !audio.micKnown) {
                audio.open = false
                audio.settle()
            }
        }
    }

    // How long a call waits for the policy at most before its page comes
    // up. A manager that has not answered by then may never: the call
    // goes on without, and what was not sent waits for it to be over.
    Timer {
        id: patience
        objectName: "patience"
        interval: 5000
        onTriggered: {
            if (audio.talking && !audio.ready) {
                audio.letIn()
            }
        }
    }

    // The ringing over: looked at once the call's state has settled, so
    // that a call answered asks for its own set before the ring's goes.
    Timer {
        id: ringOver
        objectName: "ringOver"
        interval: 0
        onTriggered: audio.settle()
    }

    // How long a switch may take: the microphone not shutting, the
    // policy not answering or the microphone not coming back. Given up
    // on, the microphone is back, and the sound where it is.
    Timer {
        id: stalled
        objectName: "stalled"
        interval: 5000
        onTriggered: audio.giveUp()
    }

    /// The reader's switch: the loudspeaker, or not.
    function choose(loud) {
        if (!audio.switchable) {
            return
        }
        audio.chosen = true
        audio.speaker = loud
    }

    onTalkingChanged: {
        settling.stop()
        stalled.stop()
        audio.switching = false
        audio.micWanted = false
        if (audio.talking) {
            audio.chosen = false
            audio.denied = audio.appId.length === 0
            audio.speaker = audio.video || audio.watching || audio.denied
            patience.restart()
            audio.lookAtRoute()
        } else {
            patience.stop()
            audio.ready = false
        }
        audio.look()
        audio.settle()
    }

    onRingingChanged: {
        if (audio.ringing) {
            ringOver.stop()
            audio.ringRefused = false
            audio.settle()
        } else {
            ringOver.restart()
        }
    }

    onWatchingChanged: {
        if (audio.watching && audio.talking && !audio.chosen) {
            audio.speaker = true
        }
    }

    onSpeakerChanged: audio.settle()
    onMicKnownChanged: audio.look()
    onMicOpenChanged: {
        if (audio.micOpen && audio.switching && audio.open && audio.micWanted) {
            audio.switching = false
            stalled.stop()
        }
        audio.look()
    }

    /// The microphone was told to go: the call is over, or the sound is
    /// about to move.
    function told() {
        return audio.open && !(audio.talking && audio.micWanted)
    }

    /// Told to go, and gone, as PulseAudio says.
    function quiet() {
        return audio.told() && audio.micKnown && !audio.micOpen
    }

    /// Wait for the microphone to shut, in whichever way there is.
    function look() {
        if (audio.quiet()) {
            if (!quieting.running) {
                quieting.start()
            }
        } else {
            quieting.stop()
        }
        if (audio.told() && !audio.micKnown) {
            if (!shutting.running) {
                shutting.start()
            }
        } else {
            shutting.stop()
        }
    }

    /// The page may have the microphone: from here on nothing is sent.
    function letIn() {
        patience.stop()
        audio.open = true
        audio.ready = true
        audio.micWanted = true
        audio.look()
    }

    /// The set the call wants, and only that, and none once it is over --
    /// unless a microphone of the call's may be open: then a switch, told
    /// to the page first. And the ring's set while the call rings.
    function settle() {
        if (audio.open) {
            if (audio.talking && audio.ready && audio.held !== 0
                    && audio.heldLoud !== audio.speaker && !audio.switching) {
                audio.startSwitch()
            }
            return
        }
        if (!audio.talking) {
            audio.letGo()
        } else if (audio.held === 0) {
            if (!audio.denied && audio.appId.length > 0) {
                audio.hold()
            }
        } else if (audio.heldLoud !== audio.speaker) {
            audio.change()
        }
        audio.ring()
        audio.pump()
    }

    /// The ring's set: asked for while the call rings, and given back
    /// once it does not -- after whatever the call's own set has asked
    /// for, which is queued before it.
    function ring() {
        if (audio.ringing) {
            if (audio.ringHeld === 0 && !audio.ringRefused && audio.appId.length > 0) {
                audio.count += 1
                var id = audio.count
                audio.ringHeld = id
                audio.send(id, "register",
                           audio.record(audio.msgRegister, id, "ringtone", audio.audioPlayback))
                audio.send(id, "acquire", audio.plain(audio.msgAcquire, id))
            }
        } else if (!ringOver.running) {
            audio.letGoRing()
        }
    }

    /// Have the page stop the microphone, and move the sound once it has.
    /// Not without the watcher to say so: the sound stays.
    function startSwitch() {
        if (!audio.micKnown) {
            audio.speaker = audio.heldLoud
            return
        }
        audio.switching = true
        audio.micWanted = false
        stalled.restart()
        audio.look()
    }

    /// A switch that did not get on: the microphone back, and the switch
    /// as the sound is.
    function giveUp() {
        if (!audio.switching) {
            return
        }
        audio.switching = false
        if (!audio.talking) {
            return
        }
        if (audio.open) {
            audio.speaker = audio.heldLoud
            audio.micWanted = true
            audio.look()
        } else {
            audio.letIn()
        }
    }

    /// Ask for the call's set: register it, name its streams, acquire it.
    function hold() {
        audio.count += 1
        var id = audio.count
        audio.held = id
        audio.heldLoud = audio.speaker
        audio.send(id, "register",
                   audio.record(audio.msgRegister, id, "aliencall", audio.callResources()))
        audio.send(id, "audio", [
            { "type": "i", "value": audio.msgAudio },
            { "type": "u", "value": id },
            { "type": "u", "value": audio.next() },
            { "type": "s", "value": "aliencall" },
            { "type": "s", "value": audio.appId },
            // Every stream of the app's, whatever its name: "*" matches
            // all of them (pulseaudio-policy-enforcement).
            { "type": "s", "value": "media.name" },
            { "type": "i", "value": 0 },
            { "type": "s", "value": "*" }
        ])
        audio.send(id, "acquire", audio.plain(audio.msgAcquire, id))
    }

    /// The set held, for the other way out: video playback added or
    /// dropped, and acquired again.
    function change() {
        var id = audio.held
        audio.heldLoud = audio.speaker
        audio.send(id, "update",
                   audio.record(audio.msgUpdate, id, "aliencall", audio.callResources()))
        audio.send(id, "acquire", audio.plain(audio.msgAcquire, id))
    }

    /// Give the call's set back.
    function letGo() {
        var id = audio.held
        audio.held = 0
        audio.drop(id)
    }

    /// Give the ring's set back.
    function letGoRing() {
        var id = audio.ringHeld
        audio.ringHeld = 0
        audio.drop(id)
    }

    /// Give set `id` back, if there is one. Unregistering is what
    /// matters; releasing first is how libresource's own clients let go.
    /// Whatever was still to be said about the set goes unsaid, and the
    /// two go out after the message already out, so a register is
    /// answered before them.
    function drop(id) {
        if (id === 0) {
            return
        }
        audio.queue = audio.queue.filter(function (message) {
            return message.id !== id
        })
        audio.send(id, "release", audio.plain(audio.msgRelease, id))
        audio.send(id, "unregister", audio.plain(audio.msgUnregister, id))
    }

    /// Queue one message about set `id`.
    function send(id, method, args) {
        audio.queue = audio.queue.concat([{ "id": id, "method": method, "args": args }])
    }

    /// Send the oldest message waiting, once every message before it is
    /// answered, as libresource's own clients do. The manager needs it:
    /// it answers a register only after asking the bus whose connection
    /// the app's is, and refuses anything else about the set that comes
    /// before then. Nothing at all while a microphone may be open. With
    /// nothing left to send in a call, the microphone's moment comes.
    function pump() {
        if (audio.waiting || audio.open) {
            return
        }
        if (audio.queue.length === 0) {
            if (audio.talking) {
                settling.restart()
            }
            return
        }
        var message = audio.queue[0]
        audio.queue = audio.queue.slice(1)
        audio.waiting = true
        manager.typedCall(message.method, message.args, function (kind, setId, number, errcod) {
            audio.waiting = false
            if (typeof errcod === "number" && errcod !== 0) {
                audio.refused(message.id)
            }
            audio.pump()
        }, function () {
            audio.waiting = false
            audio.refused(message.id)
            audio.pump()
        })
    }

    /// Refused, or no manager to answer: the set given back, and the call
    /// on the loudspeaker for good -- as long as it is still the one held:
    /// a refusal can come for a set already given back.
    function refused(id) {
        if (id === audio.held) {
            audio.letGo()
            audio.denied = true
            audio.speaker = true
        } else if (id === audio.ringHeld) {
            // Music that plays on through the ring, as it did before.
            audio.letGoRing()
            audio.ringRefused = true
        }
    }

    function next() {
        audio.reqno += 1
        return audio.reqno
    }

    /// What the call's set asks for: video playback as well for the
    /// loudspeaker.
    function callResources() {
        return audio.heldLoud ? audio.audioPlayback | audio.videoPlayback : audio.audioPlayback
    }

    /// A register or an update: the resources, the class, and who asks.
    function record(type, id, klass, resources) {
        return [
            { "type": "i", "value": type },
            { "type": "u", "value": id },
            { "type": "u", "value": audio.next() },
            // Mandatory: a route the policy will not give is better as
            // no set at all.
            { "type": "u", "value": resources },
            { "type": "u", "value": 0 },
            { "type": "u", "value": 0 },
            { "type": "u", "value": 0 },
            { "type": "s", "value": audio.appId },
            { "type": "s", "value": klass },
            { "type": "u", "value": 0 }
        ]
    }

    /// An acquire, a release or an unregister: the set alone.
    function plain(type, id) {
        return [
            { "type": "i", "value": type },
            { "type": "u", "value": id },
            { "type": "u", "value": audio.next() }
        ]
    }

    /// Whether the sound is on something plugged in or paired.
    function lookAtRoute() {
        routes.typedCall("ActiveRoutes", [], function (sink, sinkType) {
            audio.elsewhere = typeof sinkType === "number" && !(sinkType & audio.routeBuiltin)
                    && (sinkType & (audio.routeWired | audio.routeWireless)) !== 0
        }, function () {})
    }

    // ohm's resource manager, on the system bus. Open to an app with the
    // Audio permission.
    DBusInterface {
        id: manager
        objectName: "resourceManager"
        bus: DBus.SystemBus
        service: "org.maemo.resource.manager"
        path: "/org/maemo/resource/manager"
        iface: "org.maemo.resource.manager"
    }

    // ohm's route manager: where the sound is now. Read, never told.
    DBusInterface {
        id: routes
        objectName: "routes"
        bus: DBus.SystemBus
        service: "org.nemomobile.Route.Manager"
        path: "/org/nemomobile/Route/Manager"
        iface: "org.nemomobile.Route.Manager"
        signalsEnabled: true

        // AudioRouteChanged: a route came, went or became the one in
        // use. `rc` before the signal's own name: a QML function cannot
        // begin with a capital.
        function rcAudioRouteChanged(name, type) {
            if (audio.talking) {
                audio.lookAtRoute()
            }
        }
    }

    // On the way out. Nothing is sent while a microphone may be open:
    // the manager drops the set anyway once the app's connection goes,
    // which is after the process's own streams have. Otherwise what is
    // left goes now, without waiting, since nothing is answered once the
    // component is gone: an unregister that overtakes a register still
    // being answered drops the set all the same.
    Component.onDestruction: {
        if (audio.open) {
            return
        }
        audio.letGo()
        audio.letGoRing()
        var left = audio.queue
        audio.queue = []
        for (var i = 0; i < left.length; i++) {
            manager.typedCall(left[i].method, left[i].args, function () {}, function () {})
        }
    }
}
