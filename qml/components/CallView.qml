import QtQuick 2.0
import Sailfish.WebView 1.0
import Sailfish.WebEngine 1.0

/*
 * Call media page: upstream calls-webapp, served on loopback (call_host.rs),
 * run by browser engine (peer connection, codecs, echo cancel). Own component:
 * `Sailfish.WebView` missing on some releases, only this breaks. Decides
 * nothing about the call; only how engine treats the page.
 */
Item {
    id: root

    /// Call page URL.
    property string url: ""

    /// Page on screen, app in front.
    property bool shown: true

    /// Call up: page runs shown or not.
    property bool holding: false

    /// Page drawn once.
    readonly property bool loaded: view.loaded

    /// Page up once call sound routed (CallAudio.ready): earlier, mic opens
    /// before sound moves, killing sound server (CallAudio.qml). Up page stays.
    property bool allowed: true

    /// Target URL, once mic granted. Set, not bound: grant first.
    property string target: ""

    /// Page address: `127.0.0.1:port`.
    function authorityOf(where) {
        var at = ("" + where).indexOf("//")
        if (at < 0) {
            return ""
        }
        var rest = ("" + where).substring(at + 2)
        var slash = rest.indexOf("/")
        return slash < 0 ? rest : rest.substring(0, slash)
    }

    /// Put view back if page navigates; holds mic, so stays. As WebxdcPage.qml.
    function keepInside() {
        var here = root.authorityOf(view.url)
        var ours = root.authorityOf(root.target)
        if (ours.length === 0 || here.length === 0 || here === ours) {
            return
        }
        view.stop()
        view.url = Qt.binding(function () { return root.target })
    }

    /// Origin granted mic and camera while held.
    property string granted: ""

    /// Device grants. Camera opens only when switched on (call_video.js); grant
    /// keeps switch from prompting.
    readonly property var devices: ["microphone", "camera"]

    /// Grant page mic and camera before it asks. Engine prompts per origin, new
    /// port each call ("remember" fails); reader consented already. Session
    /// grant, revoked at end. Origin only: engine-wide hits webxdc apps too.
    function grant(origin) {
        if (root.granted === origin) {
            return
        }
        root.revoke()
        if (origin.length === 0) {
            return
        }
        for (var i = 0; i < root.devices.length; i++) {
            WebEngine.notifyObservers("embedui:perms", {
                "msg": "add",
                "uri": "http://" + origin,
                "type": root.devices[i],
                // ALLOW_ACTION: session only.
                "permission": 1,
                "expireType": 1
            })
        }
        root.granted = origin
    }

    function revoke() {
        if (root.granted.length === 0) {
            return
        }
        for (var i = 0; i < root.devices.length; i++) {
            WebEngine.notifyObservers("embedui:perms", {
                "msg": "remove",
                "uri": "http://" + root.granted,
                "type": root.devices[i]
            })
        }
        root.granted = ""
    }

    /// Grant origin, then point view. Not before engine up (event loop after first
    /// import, i.e. first call): earlier requests dropped with warning, call opens
    /// on mic prompt.
    function go() {
        if (!WebEngine.isInitialized()) {
            return
        }
        var where = root.allowed ? root.url : ""
        root.grant(root.authorityOf(where))
        root.target = where
    }

    Connections {
        target: WebEngine
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onInitialized: root.go()
    }

    onUrlChanged: root.go()
    onAllowedChanged: {
        if (root.allowed) {
            root.go()
        }
    }
    Component.onCompleted: root.go()
    Component.onDestruction: root.revoke()

    // `active`: WebView.qml's binding (`shown`) until page up. Hidden document
    // pauses media (other end's voice): active for the call. Never-active view
    // draws nothing (WebxdcPage.qml).
    WebView {
        id: view
        objectName: "callView"
        anchors.fill: parent
        active: root.shown || (root.holding && view.loaded)
        url: root.target
        onUrlChanged: root.keepInside()
    }
}
