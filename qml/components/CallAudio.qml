import QtQuick 2.0
import Nemo.DBus 2.0

/*
 * Call sound: earpiece (voice), loudspeaker (video, or pictures come on unless
 * reader switched; as reference clients), headset/Bluetooth (unswitchable).
 * Reader switch holds for call; busy until mic back.
 *
 * Sailfish: moving sound while call mic open kills PulseAudio (Gecko/cubeb
 * pins mic; policy asserts every call stream moved, policy-group.c
 * start_move_group). Restarted, it forgets call streams; a few crashes and
 * systemd gives up (no sound till manual restart). Headset plugged mid-call
 * too; unpreventable. So never move while mic may be open (`open`):
 * - call set acquired before mic opens (`ready`, CallView.allowed);
 * - switch: page stops mic (`micWanted`); move once MicWatch sees no
 *   recording stream (`micKnown`, `micOpen`); page reopens mic;
 * - call over: set returned once mic shut.
 * Watcher can't tell: stopped mic counts shut after a few seconds.
 *
 * Set: class `aliencall` (Sailfish non-phone call) on ohm resource manager:
 * earpiece or headset first; with video playback: loudspeaker. Names streams
 * by libpulse id (Call.audio_id) to join call group; other music pauses. ohm
 * drops sets with app's bus connection (crash restores routing). Refused, no
 * manager, or no id: loudspeaker for good.
 *
 * Ringing: set class `ringtone`, no streams (ngfd rings), moves nothing of
 * app's. Released after; answered call's set acquired first: music stays off.
 */
Item {
    id: audio

    /// Sound live: placed or answered, not over.
    property bool talking: false

    /// Call ringing here.
    property bool ringing: false

    /// Placed or answered with pictures.
    property bool video: false

    /// Pictures on screen.
    property bool watching: false

    /// Policy stream id (Call.audio_id); none: nothing asked.
    property string appId: ""

    /// Mic watcher knows; recording open (MicWatch.known, .open).
    property bool micKnown: false
    property bool micOpen: false

    /// Sound meant for loudspeaker.
    property bool speaker: false

    /// Reader switched this call: pictures stop deciding.
    property bool chosen: false

    /// Sound on headset or paired device; switch can't move it.
    property bool elsewhere: false

    /// Set refused or no id: loudspeaker for good.
    property bool denied: false

    /// Sound routed: page may come up (CallView.allowed). Till call over.
    property bool ready: false

    /// Page may hold mic: not while sound moves, not before routed.
    property bool micWanted: false

    /// Mic may be open (page had it, not surely shut). Nothing sent meanwhile.
    property bool open: false

    /// Switch in progress: tap until mic back.
    property bool switching: false

    /// Switchable: sound on phone, under call's set, mic seen shut.
    readonly property bool switchable: talking && !denied && !elsewhere && !switching
                                       && (micKnown || !open)

    /// Held set number; 0 none.
    property int held: 0

    /// Held set has video playback (loudspeaker).
    property bool heldLoud: false

    /// Ringing set number; 0 none.
    property int ringHeld: 0

    /// Ringing set refused: not asked again this ring.
    property bool ringRefused: false

    /// Last set number (manager keys sets by client + number).
    property int count: 0

    /// Last request number; manager answers by it.
    property int reqno: 0

    /// Queue, oldest first: `{ id, method, args }`.
    property var queue: []

    /// Message sent, answer pending.
    property bool waiting: false

    // libresource (res-msg.h, res-types.h): message types, resources.
    readonly property int msgRegister: 0
    readonly property int msgUnregister: 1
    readonly property int msgUpdate: 2
    readonly property int msgAcquire: 3
    readonly property int msgRelease: 4
    readonly property int msgAudio: 7
    readonly property int audioPlayback: 1
    readonly property int videoPlayback: 2

    // Route types, ohm-ext/route.h.
    readonly property int routeBuiltin: 4
    readonly property int routeWired: 8
    readonly property int routeWireless: 16

    // After acquire: policy moves sound before mic opens (ohm answers, then
    // tells PulseAudio).
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

    // Watcher: mic shut, and again moment later (streams just opening).
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

    // No mic-shut signal: time after stop till surely shut (engine stops off
    // event loop on teardown).
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

    // Max wait for policy before page up. Silent: call goes on; unsent waits
    // for call over.
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

    // Ring over: checked once state settles (answered call's set first).
    Timer {
        id: ringOver
        objectName: "ringOver"
        interval: 0
        onTriggered: audio.settle()
    }

    // Switch limit (mic not shutting, policy silent, mic not back): mic back,
    // sound stays.
    Timer {
        id: stalled
        objectName: "stalled"
        interval: 5000
        onTriggered: audio.giveUp()
    }

    /// Switch: loudspeaker or not.
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

    /// Mic told to go: call over or sound moving.
    function told() {
        return audio.open && !(audio.talking && audio.micWanted)
    }

    /// Told to go, and gone (PulseAudio).
    function quiet() {
        return audio.told() && audio.micKnown && !audio.micOpen
    }

    /// Wait for mic shut.
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

    /// Page may have mic: nothing sent.
    function letIn() {
        patience.stop()
        audio.open = true
        audio.ready = true
        audio.micWanted = true
        audio.look()
    }

    /// Call set wanted (none once over; mic may be open: page told first). Ring
    /// set while ringing.
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

    /// Ring set while ringing; call's set queued first.
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

    /// Page stops mic, then sound moves. No watcher: sound stays.
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

    /// Failed switch: mic back, switch shows sound as is.
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

    /// Acquire call set: register, name streams, acquire.
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
            // All app streams: "*" matches all.
            { "type": "s", "value": "media.name" },
            { "type": "i", "value": 0 },
            { "type": "s", "value": "*" }
        ])
        audio.send(id, "acquire", audio.plain(audio.msgAcquire, id))
    }

    /// Video playback added or dropped; acquire again.
    function change() {
        var id = audio.held
        audio.heldLoud = audio.speaker
        audio.send(id, "update",
                   audio.record(audio.msgUpdate, id, "aliencall", audio.callResources()))
        audio.send(id, "acquire", audio.plain(audio.msgAcquire, id))
    }

    /// Release call set.
    function letGo() {
        var id = audio.held
        audio.held = 0
        audio.drop(id)
    }

    /// Release ring set.
    function letGoRing() {
        var id = audio.ringHeld
        audio.ringHeld = 0
        audio.drop(id)
    }

    /// Release set `id`: release then unregister (libresource style). Queued
    /// messages dropped; these go after one already out (pending register
    /// answered first).
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

    /// Queue message about set `id`.
    function send(id, method, args) {
        audio.queue = audio.queue.concat([{ "id": id, "method": method, "args": args }])
    }

    /// Send oldest once earlier answered (as libresource): manager answers
    /// register after asking bus, refuses other ops before. Nothing while mic
    /// may be open. Queue empty in call: `letIn` mic.
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

    /// Refused or no manager: release set, loudspeaker for good. Only if still
    /// held: refusal may be for a released set.
    function refused(id) {
        if (id === audio.held) {
            audio.letGo()
            audio.denied = true
            audio.speaker = true
        } else if (id === audio.ringHeld) {
            // Music plays on through ring.
            audio.letGoRing()
            audio.ringRefused = true
        }
    }

    function next() {
        audio.reqno += 1
        return audio.reqno
    }

    /// Call set wants video playback (loudspeaker).
    function callResources() {
        return audio.heldLoud ? audio.audioPlayback | audio.videoPlayback : audio.audioPlayback
    }

    /// Register or update: resources, class, requester.
    function record(type, id, klass, resources) {
        return [
            { "type": "i", "value": type },
            { "type": "u", "value": id },
            { "type": "u", "value": audio.next() },
            // Mandatory: no set beats one whose route policy won't give.
            { "type": "u", "value": resources },
            { "type": "u", "value": 0 },
            { "type": "u", "value": 0 },
            { "type": "u", "value": 0 },
            { "type": "s", "value": audio.appId },
            { "type": "s", "value": klass },
            { "type": "u", "value": 0 }
        ]
    }

    /// Acquire, release or unregister: set alone.
    function plain(type, id) {
        return [
            { "type": "i", "value": type },
            { "type": "u", "value": id },
            { "type": "u", "value": audio.next() }
        ]
    }

    /// Sound on headset or paired device.
    function lookAtRoute() {
        routes.typedCall("ActiveRoutes", [], function (sink, sinkType) {
            audio.elsewhere = typeof sinkType === "number" && !(sinkType & audio.routeBuiltin)
                    && (sinkType & (audio.routeWired | audio.routeWireless)) !== 0
        }, function () {})
    }

    // ohm resource manager, system bus. Needs Audio permission.
    DBusInterface {
        id: manager
        objectName: "resourceManager"
        bus: DBus.SystemBus
        service: "org.maemo.resource.manager"
        path: "/org/maemo/resource/manager"
        iface: "org.maemo.resource.manager"
    }

    // ohm route manager: current sound route. Read only.
    DBusInterface {
        id: routes
        objectName: "routes"
        bus: DBus.SystemBus
        service: "org.nemomobile.Route.Manager"
        path: "/org/nemomobile/Route/Manager"
        iface: "org.nemomobile.Route.Manager"
        signalsEnabled: true

        // AudioRouteChanged; `rc` prefix: QML functions can't start capital.
        function rcAudioRouteChanged(name, type) {
            if (audio.talking) {
                audio.lookAtRoute()
            }
        }
    }

    // Exit: nothing sent while mic may be open; ohm drops set when connection
    // goes, after own streams. Else send now unanswered: unregister overtaking
    // pending register still drops set.
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
