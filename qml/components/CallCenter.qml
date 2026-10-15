import QtQuick 2.0
import Nemo.Notifications 1.0
import Nemo.DBus 2.0
import Nemo.KeepAlive 1.2
import Piirit 1.0
import "."

/*
 * The app's one call. On the window: a call belongs to no page.
 * CallPage.qml is pushed over whatever is there.
 *
 * Phone call handling as far as a non-phone app can have it. System call
 * screen and history are closed.
 *
 * - Rings via ngfd (phone's VoIP ringtone): reader's tone, volume, silent
 *   profile; stops when told or app goes. Other apps' music pauses
 *   (CallAudio.qml).
 * - Tells mce "ringing" once ringtone picked (else call-waiting beep), then
 *   "active": screen on, phone treated as in call. mce forgets if app goes.
 * - Screen on: lock screen off, app forward. Notification with Decline/Answer,
 *   for device-locked phones where page stays behind the lock.
 * - Keeps phone awake while call up: suspend would kill the sound.
 * - Unanswered call: missed-call notification (phone's category) with Call back.
 */
Item {
    id: center

    /// The call.
    readonly property alias call: call

    /// Under way: ringing, setting up, or live.
    readonly property bool busy: call.state !== "" && call.state !== "ended"

    /// Last failure.
    property string errorMessage: ""

    /// Page stack from window; tests supply own.
    property var stack: null

    /// Page call shows on, while on stack.
    property Item page: null

    /// Wanted while last call's page leaves; pushed once gone. Still shows if
    /// declined in that window (#110).
    property bool pageOwed: false

    /// Ringtone id playing (ngfd); 0 none.
    property int ringId: 0

    /// Ringtone picked (or timed out): mce may be told. Not before: ngfd follows
    /// mce call state, and `voip_ringtone` plays a call-waiting beep if mce says a
    /// call is on (ringing counts) when played (#98).
    property bool ringSettled: false

    /// Fell back to `ringtone`, tried once.
    property bool ringFellBack: false

    /// Lock off once screen on, so page shows (#99).
    property bool unlockWanted: false

    /// Bring app forward. Window connects to `activate`.
    signal raise()

    /// Open chat from missed-call notification.
    signal chatRequested(int accountId, int chatId)

    Call {
        id: call
        objectName: "call"
        // Main lens: back camera, as platform labels.
        back_lenses: String(Settings.backLensLabels)
        onRinging: center.ring()
        onEnded: center.over(reason)
        onError: center.errorMessage = message
    }

    /// App not in front: camera let go, no pictures (calls.rs `set_background`).
    /// Writable: tests (no app in front) set it.
    property bool away: Qt.application.state !== Qt.ApplicationActive
    onAwayChanged: call.set_background(center.away)
    Component.onCompleted: call.set_background(center.away)

    /// Pictures on call screen.
    readonly property bool watching: center.busy && (call.local_video || call.remote_video)

    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onCore_event: {
            call.handle_event(context_id, kind, payload_json)
        }
    }

    /// Call chat, camera on if video. False if already in call.
    function place(accountId, chatId, video) {
        if (center.busy) {
            return false
        }
        if (!call.place(accountId, chatId, video === true)) {
            return false
        }
        center.show()
        return true
    }

    /// Take up call still ringing from its row, once core confirms.
    function pickUp(accountId, chatId, messageId) {
        if (center.busy) {
            center.show()
            return
        }
        call.pick_up(accountId, chatId, messageId)
    }

    /// Show call, page to front.
    function show() {
        if (!center.busy && call.state !== "ended") {
            return
        }
        if (center.page !== null && center.stack !== null) {
            // Last call's page still going: this one shows after.
            if (center.page.leaving === true) {
                center.pageOwed = true
            }
            return
        }
        pushLater.restart()
    }

    /// Call came in: ring and show.
    function ring() {
        center.unlockWanted = true
        center.raise()
        center.show()
        ringNote.ring()
        center.startRinging()
    }

    /// Ring with `voip_ringtone` (event for non-phone calls); repeats till stopped.
    function startRinging() {
        if (center.ringId !== 0) {
            return
        }
        center.ringFellBack = false
        // ngfd silent: mce told anyway, a moment later (screen matters more).
        ringSettle.restart()
        center.play("voip_ringtone", {})
    }

    /// Play ringtone event; keep ngfd's number.
    function play(event, properties) {
        feedback.typedCall("Play", [
            { "type": "s", "value": event },
            { "type": "a{sv}", "value": properties }
        ], function (id) {
            center.settleRing()
            if (call.state !== "ringing") {
                // Stopped before ngfd answered: stop now.
                center.stopRinging(id)
            } else if (id === 0) {
                // Refused: no such event.
                center.fallBack()
            } else {
                center.ringId = id
            }
        }, function () {
            center.settleRing()
        })
    }

    /// Phone's `ringtone` (voicecall, non-SIM calls), for when `voip_ringtone`
    /// fails. Once per call.
    function fallBack() {
        if (center.ringFellBack || call.state !== "ringing") {
            return
        }
        center.ringFellBack = true
        center.ringId = 0
        center.play("ringtone", { "type": "voip" })
    }

    /// ngfd done with ringtone: mce may be told. Late answer isn't next call's.
    function settleRing() {
        ringSettle.stop()
        if (call.state === "ringing") {
            center.ringSettled = true
        }
    }

    Timer {
        id: ringSettle
        objectName: "ringSettle"
        interval: 1000
        onTriggered: center.settleRing()
    }

    /// Stop ringtone, playing or named.
    function stopRinging(id) {
        var which = id !== undefined ? id : center.ringId
        if (id === undefined) {
            center.ringId = 0
        }
        if (which !== 0) {
            feedback.typedCall("Stop", [{ "type": "u", "value": which }],
                               function () {}, function () {})
        }
    }

    function answer() {
        center.raise()
        center.show()
        call.answer()
    }

    function decline() {
        call.hang_up()
    }

    /// Call over: ringing stops; unanswered call left as missed.
    function over(reason) {
        center.stopRinging()
        ringNote.withdraw()
        if (reason === "missed") {
            missedNote.missed(call.account_id, call.chat_id, call.peer_name)
        }
    }

    // Page pushed when stack takes it (push mid-transition refused, stderr only).
    // See PendingNavigation.qml.
    Timer {
        id: pushLater
        objectName: "pushLater"
        interval: 50
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            if (center.stack === null || center.page !== null) {
                pushLater.stop()
                return
            }
            if (center.stack.busy === true) {
                return
            }
            pushLater.stop()
            center.pageOwed = false
            var shown = center.stack.push(Qt.resolvedUrl("../pages/CallPage.qml"),
                                          { call: call, audio: callAudio })
            if (shown) {
                center.page = shown
            }
        }
    }

    // Forgotten when gone; next call pushes own.
    Connections {
        target: center.page
        ignoreUnknownSignals: true
        onDestroyed: center.page = null
    }

    // Pushed now if came in during old page's exit; also owed for calls ended
    // meanwhile (rang, declined). Gone page reads null here whether or not the
    // handler above ran.
    onPageChanged: {
        if (center.page === null && (center.busy || center.pageOwed)) {
            center.pageOwed = false
            center.show()
        }
    }

    Connections {
        target: call
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onState_changed: {
            if (call.state !== "ringing") {
                // Answered, declined or gone: ringtone stops.
                center.stopRinging()
                ringNote.withdraw()
                ringSettle.stop()
                center.ringSettled = false
                center.unlockWanted = false
            } else if (center.page === null) {
                // Taken from row: shown, no ring.
                center.show()
            }
        }
        // Caller read after ring starts; shown once known.
        onCall_changed: ringNote.name()
    }

    // ngfd (system bus): sounds, vibration, lights. Any app may ask an event.
    DBusInterface {
        id: feedback
        objectName: "feedback"
        bus: DBus.SystemBus
        service: "com.nokia.NonGraphicFeedback1.Backend"
        path: "/com/nokia/NonGraphicFeedback1"
        iface: "com.nokia.NonGraphicFeedback1"
        signalsEnabled: true

        // ngfd Status: 0 = failed; VoIP failure while ringing falls back to phone
        // ringtone. `rc`, signal name: QML function can't start capital; `status`
        // taken by interface property.
        function rcStatus(id, status) {
            if (status === 0 && id !== 0 && id === center.ringId) {
                center.fallBack()
            }
        }
    }

    /// mce state: "ringing", "active" (call up), else "none".
    readonly property string mceState: call.state === "ringing"
                                       ? (center.ringSettled ? "ringing" : "none")
                                       : center.busy ? "active" : "none"
    /// Whether mce was told anything, so app start's "none" isn't sent.
    property bool mceTold: false

    onMceStateChanged: {
        if (center.mceState === "none" && !center.mceTold) {
            return
        }
        center.mceTold = true
        mce.typedCall("req_call_state_change", [
            { "type": "s", "value": center.mceState },
            { "type": "s", "value": "normal" }
        ], function () {}, function () {})
        if (center.mceState === "ringing") {
            // Already lit: mce says nothing more.
            mce.typedCall("get_display_status", [], function (status) {
                center.displayIs(status)
            }, function () {})
        }
    }

    /// Screen on/off/dimmed per mce. On while ringing: lock screen taken off
    /// (raised behind it, page showed a moment then got covered; #99). mce lights
    /// screen for ringing call; lock lifted only once on, so pocket phone (mce
    /// keeps screen off) stays locked. mce relocks when call over. Device lock
    /// (code, fingerprint) untouched: page waits behind it.
    function displayIs(status) {
        if (status !== "on" || !center.unlockWanted || call.state !== "ringing") {
            return
        }
        center.unlockWanted = false
        mce.typedCall("req_tklock_mode_change", [
            { "type": "s", "value": "unlocked" }
        ], function () {}, function () {})
        center.raise()
    }

    // mce (system bus): display, lock, proximity. Told of call: lights screen,
    // treats phone as in call.
    DBusInterface {
        id: mce
        objectName: "mce"
        bus: DBus.SystemBus
        service: "com.nokia.mce"
        path: "/com/nokia/mce/request"
        iface: "com.nokia.mce.request"
    }

    // mce signals: screen going on for ringing call.
    DBusInterface {
        objectName: "mceSignals"
        bus: DBus.SystemBus
        service: "com.nokia.mce"
        path: "/com/nokia/mce/signal"
        iface: "com.nokia.mce.signal"
        signalsEnabled: true

        function display_status_ind(status) {
            center.displayIs(status)
        }
    }

    // Call sound: earpiece (voice), loudspeaker (video), or switch. Held from
    // carry-sound till over; page waits (CallPage.qml). Ringing: other music yields.
    CallAudio {
        id: callAudio
        objectName: "callAudio"
        talking: center.busy && call.state !== "ringing"
        ringing: call.state === "ringing"
        video: call.has_video
        watching: center.watching
        appId: call.audio_id()
        micKnown: micWatch.known
        micOpen: micWatch.open
        onMicWantedChanged: call.set_microphone(callAudio.micWanted)
    }

    // Mic open (PulseAudio)? Call sound waits for it. Connected only then.
    MicWatch {
        id: micWatch
        objectName: "micWatch"
        active: callAudio.talking || callAudio.open
    }

    // Awake while call up: suspend takes call's sound. Lit while ringing.
    KeepAlive {
        objectName: "callKeepAlive"
        enabled: center.busy
    }

    // Lit while pictures on: no touch in video call. Not while app away.
    DisplayBlanking {
        objectName: "callDisplay"
        preventBlanking: call.state === "ringing" || (center.watching && !center.away)
    }

    // Notification taps come back here. Own path under chat notifications' name
    // (Notifier.qml), so neither drops the other's calls.
    DBusAdaptor {
        objectName: "callAdaptor"
        path: "/call"
        iface: "piirit.piirit.Call"
        xml: "  <interface name=\"piirit.piirit.Call\">\n" +
             "    <method name=\"show\"/>\n" +
             "    <method name=\"answer\"/>\n" +
             "    <method name=\"decline\"/>\n" +
             "    <method name=\"showChat\">\n" +
             "      <arg name=\"accountId\" type=\"i\" direction=\"in\"/>\n" +
             "      <arg name=\"chatId\" type=\"i\" direction=\"in\"/>\n" +
             "    </method>\n" +
             "    <method name=\"callBack\">\n" +
             "      <arg name=\"accountId\" type=\"i\" direction=\"in\"/>\n" +
             "      <arg name=\"chatId\" type=\"i\" direction=\"in\"/>\n" +
             "    </method>\n" +
             "  </interface>\n"

        function show() {
            center.raise()
            center.show()
        }
        function answer() {
            center.answer()
        }
        function decline() {
            center.decline()
        }
        function showChat(accountId, chatId) {
            missedNote.close()
            center.raise()
            center.chatRequested(accountId, chatId)
        }
        function callBack(accountId, chatId) {
            missedNote.close()
            center.raise()
            center.place(accountId, chatId)
        }
    }

    /// One remote action on own object.
    function action(name, label, method, args) {
        return {
            "name": name,
            "displayName": label,
            "service": "piirit.piirit",
            "path": "/call",
            "iface": "piirit.piirit.Call",
            "method": method,
            "arguments": args
        }
    }

    // Ringing call on lock screen. Critical: only urgency that gets through previews.
    // No category, no sound (ngfd plays ringtone).
    Notification {
        id: ringNote
        objectName: "ringNote"
        appName: "Piirit"
        appIcon: "harbour-piirit"
        // By name: Qt 5.6 checks enum-bound number against names, refuses file;
        // this is the window's file, so window fails too.
        urgency: Notification.Critical

        /// Published, not taken down.
        property bool up: false

        function ring() {
            ringNote.summary = call.peer_name
            // Phone or camera first: says kind before read.
            ringNote.body = call.has_video ? "🎥 " + qsTr("Incoming video call")
                                           : "📞 " + qsTr("Incoming call")
            ringNote.previewSummary = ringNote.summary
            ringNote.previewBody = ringNote.body
            ringNote.timestamp = new Date()
            ringNote.remoteActions = [
                center.action("default", "", "show", []),
                center.action("decline", qsTr("Decline"), "decline", []),
                center.action("answer", qsTr("Answer"), "answer", [])
            ]
            ringNote.up = true
            ringNote.publish()
        }

        /// Caller's name, once core gives it; raised without.
        function name() {
            if (!ringNote.up || ringNote.summary === call.peer_name) {
                return
            }
            ringNote.summary = call.peer_name
            ringNote.previewSummary = ringNote.summary
            ringNote.publish()
        }

        function withdraw() {
            ringNote.up = false
            ringNote.close()
        }
    }

    // Missed call, phone's category. One at a time, latest; chat holds each as
    // own row.
    Notification {
        id: missedNote
        objectName: "missedNote"
        category: "x-nemo.call.missed"
        appName: "Piirit"
        appIcon: "harbour-piirit"

        function missed(accountId, chatId, name) {
            missedNote.summary = name
            missedNote.body = "📞 " + qsTr("Missed call")
            missedNote.previewSummary = missedNote.summary
            missedNote.previewBody = missedNote.body
            missedNote.timestamp = new Date()
            missedNote.remoteActions = [
                center.action("default", "", "showChat", [accountId, chatId]),
                center.action("callBack", qsTr("Call back"), "callBack", [accountId, chatId])
            ]
            missedNote.publish()
        }
    }
}
