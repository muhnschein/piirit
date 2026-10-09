import QtQuick 2.0
import Nemo.Notifications 1.0
import Nemo.DBus 2.0
import Nemo.KeepAlive 1.2
import Piirit 1.0

/*
 * The app's one call: the window's, because a call belongs to no page.
 *
 * A call can come in whatever is on screen, on whichever profile, and a
 * page the reader leaves is not a call they have hung up. So the call
 * object lives here, beside the window, and the page that shows it
 * (CallPage.qml) is pushed over whatever is there and handed it.
 *
 * What is here is as much of the phone's own call handling as an app
 * that is not the phone app can have. The system call screen and the
 * call history are closed to it (docs/CALLS.md); what is open is below.
 *
 * - It rings with the phone's own VoIP ringtone, played by the feedback
 *   daemon (ngfd) that plays every sound the phone makes: the tone, the
 *   volume and the silent profile are the reader's own settings, and it
 *   stops when told or when the app goes.
 * - It tells the display daemon (mce) that a call is ringing -- once
 *   the ringtone has been picked, which mce's call state would turn into
 *   a call-waiting beep -- and then that one is up: the screen comes on
 *   for a call as it does for a phone call, and mce treats the phone as
 *   being in one. mce forgets it by itself if the app goes.
 * - Once the screen is on, it takes the lock screen off and brings the
 *   app forward, so the call's page is what the reader sees. It raises a
 *   notification too, with Decline and Answer on it, for a phone whose
 *   device lock keeps the page behind it.
 * - It keeps the phone awake while a call is up: a phone that suspends
 *   with its screen off takes the call's sound with it.
 * - A call that rang unanswered stays behind in the notification area as
 *   a missed call, in the phone's own category for one, with Call back
 *   on it.
 */
Item {
    id: center

    /// The call.
    readonly property alias call: call

    /// A call is under way: ringing, being set up, or in progress.
    readonly property bool busy: call.state !== "" && call.state !== "ended"

    /// What the last thing to fail said, for whoever shows it.
    property string errorMessage: ""

    /// Where the page stack is, handed over by the window: a
    /// `pageStack` is Silica's, and a test hands in its own.
    property var stack: null

    /// The page the call is shown on, while it is on the stack.
    property Item page: null

    /// A page was wanted while the last call's page was still on its way
    /// out, and is still owed: pushed once that page has gone, whatever
    /// the call has become by then. A call can ring and be declined in
    /// that window, and its page still says so (#110).
    property bool pageOwed: false

    /// The ringtone playing, as ngfd numbers it; 0 for none.
    property int ringId: 0

    /// ngfd has picked the ringtone for the call ringing now, or has
    /// been given long enough to: mce may be told the call is ringing.
    /// Not before. ngfd follows mce's call state, and the phone's
    /// `voip_ringtone` event plays a short call-waiting beep rather than
    /// the ringtone when mce already says a call is on -- ringing counts
    /// -- at the moment it is played (#98).
    property bool ringSettled: false

    /// The ringtone for this call has fallen back from `voip_ringtone`
    /// to the phone's own `ringtone`, which is tried once.
    property bool ringFellBack: false

    /// The lock screen is to be taken off for this call once the screen
    /// is on, so its page is what the reader sees (#99).
    property bool unlockWanted: false

    /// Bring the app forward. The window connects this to `activate`.
    signal raise()

    /// Open a chat, from a missed call's notification.
    signal chatRequested(int accountId, int chatId)

    Call {
        id: call
        objectName: "call"
        onRinging: center.ring()
        onEnded: center.over(reason)
        onError: center.errorMessage = message
    }

    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onCore_event: {
            call.handle_event(context_id, kind, payload_json)
        }
    }

    /// Call a chat. False when the app is in a call already, which the
    /// caller says to the reader.
    function place(accountId, chatId) {
        if (center.busy) {
            return false
        }
        // Audio only: see calls.rs.
        if (!call.place(accountId, chatId, false)) {
            return false
        }
        center.show()
        return true
    }

    /// Take up a call still ringing from its row: the core is asked
    /// whether it still rings, and the call page comes up once it says.
    function pickUp(accountId, chatId, messageId) {
        if (center.busy) {
            center.show()
            return
        }
        call.pick_up(accountId, chatId, messageId)
    }

    /// Show the call, bringing its page back to the front.
    function show() {
        if (!center.busy && call.state !== "ended") {
            return
        }
        if (center.page !== null && center.stack !== null) {
            // The last call's page, still going: this call's comes once
            // it has gone. Not this call's own page, which is shown.
            if (center.page.leaving === true) {
                center.pageOwed = true
            }
            return
        }
        pushLater.restart()
    }

    /// A call came in: ring, and show it.
    function ring() {
        center.unlockWanted = true
        center.raise()
        center.show()
        ringNote.ring()
        center.startRinging()
    }

    /// Have the phone ring. `voip_ringtone` is the event the phone's own
    /// call handling plays for a call that is not a phone call, and it
    /// repeats until it is stopped.
    function startRinging() {
        if (center.ringId !== 0) {
            return
        }
        center.ringFellBack = false
        // Should ngfd not answer, mce is told all the same, a moment
        // later: the screen coming on matters more than the tone.
        ringSettle.restart()
        center.play("voip_ringtone", {})
    }

    /// Play a ringtone event, and keep the number ngfd gives it.
    function play(event, properties) {
        feedback.typedCall("Play", [
            { "type": "s", "value": event },
            { "type": "a{sv}", "value": properties }
        ], function (id) {
            center.settleRing()
            if (call.state !== "ringing") {
                // Stopped before ngfd had answered: stopped at once.
                center.stopRinging(id)
            } else if (id === 0) {
                // Refused: no such event on this phone.
                center.fallBack()
            } else {
                center.ringId = id
            }
        }, function () {
            center.settleRing()
        })
    }

    /// The phone's own ringtone, as voicecall plays it for a call that
    /// is not a SIM call: for a phone whose `voip_ringtone` would not
    /// play. Once per call.
    function fallBack() {
        if (center.ringFellBack || call.state !== "ringing") {
            return
        }
        center.ringFellBack = true
        center.ringId = 0
        center.play("ringtone", { "type": "voip" })
    }

    /// ngfd has had its say about the ringtone: mce may be told now. Only
    /// for a call still ringing; an answer that comes after the call has
    /// gone is not the next call's.
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

    /// Stop the ringtone, the one playing or the one named.
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

    /// The call is over: the ringing stops, and a call that rang here
    /// unanswered is left behind as a missed call.
    function over(reason) {
        center.stopRinging()
        ringNote.withdraw()
        if (reason === "missed") {
            missedNote.missed(call.account_id, call.chat_id, call.peer_name)
        }
    }

    // The call page, pushed when the stack will take it: a call that
    // comes in during a page transition would otherwise be refused, and
    // say so on stderr alone. See PendingNavigation.qml.
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
                                          { call: call })
            if (shown) {
                center.page = shown
            }
        }
    }

    // Forgotten when it goes, so the next call pushes one of its own.
    Connections {
        target: center.page
        ignoreUnknownSignals: true
        onDestroyed: center.page = null
    }

    // And pushed now, for a call that came in while the last page was on
    // its way out: it was not shown then, the old page being there. Also
    // for one that has ended meanwhile -- rang and was declined before
    // the old page had gone -- which is owed its page all the same. A
    // page that has gone reads as null here whether or not the handler
    // above ran.
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
                // Answered, declined, or gone some other way: the
                // ringtone stops the moment the call stops ringing,
                // whichever way that happened.
                center.stopRinging()
                ringNote.withdraw()
                ringSettle.stop()
                center.ringSettled = false
                center.unlockWanted = false
            } else if (center.page === null) {
                // A call taken up from its row is ringing without having
                // rung: shown, and nothing more.
                center.show()
            }
        }
        // Who is calling is read after the call has started ringing: the
        // notification says it once it is known.
        onCall_changed: ringNote.name()
    }

    // ngfd, on the system bus: the sounds, vibration and lights the phone
    // makes. Every app may ask it to play an event.
    DBusInterface {
        id: feedback
        objectName: "feedback"
        bus: DBus.SystemBus
        service: "com.nokia.NonGraphicFeedback1.Backend"
        path: "/com/nokia/NonGraphicFeedback1"
        iface: "com.nokia.NonGraphicFeedback1"
        signalsEnabled: true

        // ngfd's Status signal: what became of an event it is playing, 0
        // being "failed". The VoIP ringtone failing while the call still
        // rings falls back to the phone's own ringtone. `rc` and the
        // signal's own name: a QML function cannot begin with a capital,
        // and `status` is taken by the interface's own property.
        function rcStatus(id, status) {
            if (status === 0 && id !== 0 && id === center.ringId) {
                center.fallBack()
            }
        }
    }

    /// What mce is told about the call: "ringing" while it rings here,
    /// "active" while one is up either way, "none" otherwise.
    readonly property string mceState: call.state === "ringing"
                                       ? (center.ringSettled ? "ringing" : "none")
                                       : center.busy ? "active" : "none"
    /// Whether mce has been told anything, so that the first "none" --
    /// the app starting -- is not sent for nothing.
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
            // Lit already, mce says nothing more about the screen: asked.
            mce.typedCall("get_display_status", [], function (status) {
                center.displayIs(status)
            }, function () {})
        }
    }

    /// The screen is on, off or dimmed, as mce says it. On, with a call
    /// ringing here, the lock screen is taken off: raised behind it, the
    /// call's page is shown for a moment and then covered again (#99).
    /// mce turns the screen on for a ringing call, and takes the lock
    /// off only once it is on -- so a phone held in a pocket, whose
    /// screen mce keeps off, stays locked. mce puts the lock back once
    /// the call is over, as it does for a phone call. A device lock, a
    /// code or a fingerprint, is not touched: the phone asks for it, and
    /// the call's page is behind it.
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

    // mce, on the system bus: the display, the lock and the proximity
    // blanking. Told of a call, it lights the screen for one that rings
    // and treats the phone as being in one, as it does for a phone call.
    DBusInterface {
        id: mce
        objectName: "mce"
        bus: DBus.SystemBus
        service: "com.nokia.mce"
        path: "/com/nokia/mce/request"
        iface: "com.nokia.mce.request"
    }

    // mce's signals: the screen going on, for a call ringing while it was
    // off.
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

    // Awake while a call is up: a phone that suspends takes the call's
    // sound with it. Lit while one rings, which is when the reader has
    // to see it.
    KeepAlive {
        objectName: "callKeepAlive"
        enabled: center.busy
    }

    DisplayBlanking {
        objectName: "callDisplay"
        preventBlanking: call.state === "ringing"
    }

    // A tap on either notification comes back here. On a path of its own
    // under the name the chat notifications own (Notifier.qml): the
    // object is published without taking the name, so neither of them
    // letting go of it takes the other's calls with it.
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

    /// One remote action on this component's own object.
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

    // The call that is ringing, on the lock screen and over whatever app
    // is in front. Critical, the one urgency the platform lets through
    // every preview setting, as a ringing phone gets through. No category,
    // and so no sound of its own: the ringtone is ngfd's, above.
    Notification {
        id: ringNote
        objectName: "ringNote"
        appName: "Piirit"
        appIcon: "harbour-piirit"
        // By name. Qt 5.6 checks a number bound to an enum property
        // against the enum's names, finds none called "2", and refuses the
        // file -- and this file is the window's, so the window with it.
        urgency: Notification.Critical

        /// Published, and not yet taken down.
        property bool up: false

        function ring() {
            ringNote.summary = call.peer_name
            // A phone first: the line says it is a call before it is read.
            ringNote.body = "📞 " + qsTr("Incoming call")
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

        /// The caller's name, once the core has said it: the call rings
        /// before it is read, and the notification was raised without it.
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

    // A call that rang here unanswered, in the phone's own category for
    // one. One at a time, the latest: the chat itself holds every missed
    // call as a row of its own.
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
