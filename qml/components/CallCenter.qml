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
 * - It tells the display daemon (mce) that a call is ringing, and then
 *   that one is up: the screen comes on for a call as it does for a
 *   phone call, and mce treats the phone as being in one. mce forgets
 *   it by itself if the app goes.
 * - It raises a notification the lock screen shows, with Decline and
 *   Answer on it, and brings the app forward.
 * - It keeps the phone awake while a call is up: a phone that suspends
 *   with its screen off takes the call's sound with it.
 * - A call that rang unanswered stays behind in the notification area as
 *   a missed call, in the phone's own category for one, with Call back
 *   on it.
 *
 * Only while calls are on (`enabled`, which the window binds to the
 * setting); a call already under way is followed to its end whatever
 * the setting has become meanwhile.
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

    /// The ringtone playing, as ngfd numbers it; 0 for none.
    property int ringId: 0

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
            if (center.enabled || center.busy) {
                call.handle_event(context_id, kind, payload_json)
            }
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
            return
        }
        pushLater.restart()
    }

    /// A call came in: ring, and show it.
    function ring() {
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
        feedback.typedCall("Play", [
            { "type": "s", "value": "voip_ringtone" },
            { "type": "a{sv}", "value": {} }
        ], function (id) {
            // Stopped before ngfd had answered: stopped at once.
            if (call.state === "ringing") {
                center.ringId = id
            } else {
                center.stopRinging(id)
            }
        }, function () {})
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
        ringNote.close()
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
    // its way out: it was not shown then, the old page being there. A
    // page that has gone reads as null here whether or not the handler
    // above ran.
    onPageChanged: {
        if (center.page === null && center.busy) {
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
                ringNote.close()
            } else if (center.page === null) {
                // A call taken up from its row is ringing without having
                // rung: shown, and nothing more.
                center.show()
            }
        }
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
    }

    /// What mce is told about the call: "ringing" while it rings here,
    /// "active" while one is up either way, "none" otherwise.
    readonly property string mceState: call.state === "ringing" ? "ringing"
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
            ringNote.publish()
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
