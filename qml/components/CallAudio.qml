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
 * - The reader switches between the two at any time (the call page's
 *   loudspeaker switch), and that holds for the rest of the call.
 * - A headset, headphones or a Bluetooth device takes the sound by
 *   itself, and the switch has nothing to move it from.
 *
 * How: for as long as the call carries sound, the app holds a resource
 * set of class `aliencall` with the phone's audio policy (ohm's resource
 * manager, on the system bus): the class Sailfish has for calls that are
 * not phone calls, which the policy routes as a call -- the earpiece
 * first, or the loudspeaker when the same set also holds video playback,
 * and a headset or a paired device before either. The set names the
 * app's streams by the id libpulse gives them (Call.audio_id), so the
 * call's sound joins the call group rather than being held back as any
 * other app's sound is while a call is on.
 *
 * Nothing is written that outlives the app. ohm drops a client's sets
 * the moment its bus connection goes, so a crash or a kill puts the sound
 * back where it was without anybody doing anything, and closing the app
 * always does. A set that cannot be had whole -- refused, or its streams
 * not named -- is let go of at once: half of one would keep the call's
 * own sound back. docs/CALLS.md, "Earpiece and loudspeaker".
 */
Item {
    id: audio

    /// The call carries sound: placed or answered, and not over.
    property bool talking: false

    /// The call was placed or answered with pictures.
    property bool video: false

    /// There are pictures on the call's screen.
    property bool watching: false

    /// The id the policy knows this app's streams by (Call.audio_id);
    /// with none, nothing is asked for.
    property string appId: ""

    /// The sound is meant for the loudspeaker rather than the earpiece.
    property bool speaker: false

    /// The reader switched it, this call: the pictures no longer decide.
    property bool chosen: false

    /// The sound is on something plugged in or paired, as the policy
    /// says: the loudspeaker switch has nothing to move it from.
    property bool elsewhere: false

    /// The resource set held for this call, by its number; 0 for none.
    property int held: 0

    /// The last set's number. Each call's set gets a new one: the
    /// manager knows a set by the client and its number.
    property int count: 0

    /// The last request's number, which the manager answers by.
    property int reqno: 0

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

    /// The reader's switch: the loudspeaker, or not.
    function choose(loud) {
        audio.chosen = true
        audio.speaker = loud
    }

    onTalkingChanged: {
        if (audio.talking) {
            audio.hold()
        } else {
            audio.letGo()
        }
    }

    onWatchingChanged: {
        if (audio.watching && audio.talking && !audio.chosen) {
            audio.speaker = true
        }
    }

    // The loudspeaker is the same set holding video playback too: the
    // policy keeps a call's sound off the earpiece while it does.
    onSpeakerChanged: {
        var id = audio.held
        if (id === 0) {
            return
        }
        audio.send(id, "update", audio.record(audio.msgUpdate, id))
        audio.send(id, "acquire", audio.plain(audio.msgAcquire, id))
    }

    /// Ask for the call's set: register it, name its streams, acquire it.
    function hold() {
        audio.chosen = false
        audio.speaker = audio.video || audio.watching
        audio.lookAtRoute()
        if (audio.held !== 0 || audio.appId.length === 0) {
            return
        }
        audio.count += 1
        var id = audio.count
        audio.held = id
        audio.send(id, "register", audio.record(audio.msgRegister, id))
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

    /// Give the set back. Unregistering is what matters; releasing first
    /// is how libresource's own clients let go.
    function letGo() {
        var id = audio.held
        if (id === 0) {
            return
        }
        audio.held = 0
        manager.typedCall("release", audio.plain(audio.msgRelease, id),
                          function () {}, function () {})
        manager.typedCall("unregister", audio.plain(audio.msgUnregister, id),
                          function () {}, function () {})
    }

    /// Send one message about set `id`. Should the manager refuse it, or
    /// not be there to answer, the set is let go of -- as long as it is
    /// still the one held. The answer comes later, after whatever else
    /// was sent meanwhile: a set's messages go out together, and the
    /// first refusal lets go of it.
    function send(id, method, args) {
        manager.typedCall(method, args, function (kind, setId, number, errcod) {
            if (typeof errcod === "number" && errcod !== 0) {
                audio.refused(id)
            }
        }, function () {
            audio.refused(id)
        })
    }

    function refused(id) {
        if (id === audio.held) {
            audio.letGo()
        }
    }

    function next() {
        audio.reqno += 1
        return audio.reqno
    }

    /// A register or an update: the resources, and who asks.
    function record(type, id) {
        return [
            { "type": "i", "value": type },
            { "type": "u", "value": id },
            { "type": "u", "value": audio.next() },
            // All of them mandatory: a loudspeaker the policy will not
            // give is better as no set at all than as the earpiece.
            { "type": "u", "value": audio.audioPlayback | (audio.speaker ? audio.videoPlayback : 0) },
            { "type": "u", "value": 0 },
            { "type": "u", "value": 0 },
            { "type": "u", "value": 0 },
            { "type": "s", "value": audio.appId },
            { "type": "s", "value": "aliencall" },
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

    // On the way out, with a call still up. Belt and braces: the manager
    // drops the set anyway once the app's connection goes.
    Component.onDestruction: audio.letGo()
}
