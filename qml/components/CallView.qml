import QtQuick 2.0
import Sailfish.WebView 1.0
import Sailfish.WebEngine 1.0

/*
 * The page a call's media runs in: upstream's calls-webapp, served by the
 * shim on a loopback address of its own (call_host.rs), in the browser
 * engine that has the peer connection, the codecs and the echo
 * cancellation.
 *
 * A component of its own, loaded by the call page, for the reason the
 * webxdc page is pushed by URL: `Sailfish.WebView` is the one type in
 * this tree that resolves only on a release that ships the browser
 * engine. Missing, it costs this and nothing else -- a call that rings
 * can still be seen, and declined, on a phone that could never have
 * answered it.
 *
 * Nothing here decides anything about the call. The page carries its own
 * controls -- the red button, the microphone -- and what they do reaches
 * the core through the host; the call page around this draws who and
 * how. What is decided here is how the engine treats the page, in two
 * places where a call is not an ordinary page.
 */
Item {
    id: root

    /// Where the call's page is, command and all. Empty for none.
    property string url: ""

    /// Whether the page is the one on screen with the app in front: when
    /// an ordinary page would be running.
    property bool shown: true

    /// Whether the call is still up. While it is, the page runs whether
    /// or not it is shown.
    property bool holding: false

    /// The page has drawn at least once.
    readonly property bool loaded: view.loaded

    /// Where the view goes: `url`, once the microphone has been granted
    /// to it. Set rather than bound, so the grant is always first.
    property string target: ""

    /// The address the page is served on, `127.0.0.1:port`.
    function authorityOf(where) {
        var at = ("" + where).indexOf("//")
        if (at < 0) {
            return ""
        }
        var rest = ("" + where).substring(at + 2)
        var slash = rest.indexOf("/")
        return slash < 0 ? rest : rest.substring(0, slash)
    }

    /// Put the view back if the page takes itself somewhere else. The
    /// page never does -- it changes its own hash, which is not moving --
    /// but the view is one that holds a microphone, and it stays where
    /// it was put. See WebxdcPage.qml, which does the same for an app.
    function keepInside() {
        var here = root.authorityOf(view.url)
        var ours = root.authorityOf(root.target)
        if (ours.length === 0 || here.length === 0 || here === ours) {
            return
        }
        view.stop()
        view.url = Qt.binding(function () { return root.target })
    }

    /// The origin the microphone is granted to, while it is.
    property string granted: ""

    /// Grant the call's own page the microphone before it asks.
    ///
    /// The engine asks the reader about every page that wants the
    /// microphone, per origin, and the call's origin is a new port every
    /// call -- so "remember" never would, and every call would open on a
    /// browser's permission prompt. The reader has already said: they
    /// placed the call, or answered it. So the page is granted it for the
    /// session, exactly as the prompt's own allow does, and it is taken
    /// back when the call goes. Only this origin: the engine-wide switch
    /// that would do the same would do it for every webxdc app as well.
    function grant(origin) {
        if (root.granted === origin) {
            return
        }
        root.revoke()
        if (origin.length === 0) {
            return
        }
        WebEngine.notifyObservers("embedui:perms", {
            "msg": "add",
            "uri": "http://" + origin,
            "type": "microphone",
            // ALLOW_ACTION, for the session: the prompt's own answer.
            "permission": 1,
            "expireType": 1
        })
        root.granted = origin
    }

    function revoke() {
        if (root.granted.length === 0) {
            return
        }
        WebEngine.notifyObservers("embedui:perms", {
            "msg": "remove",
            "uri": "http://" + root.granted,
            "type": "microphone"
        })
        root.granted = ""
    }

    /// Grant the page's origin, then point the view at the page: before
    /// the page is there, it cannot ask.
    ///
    /// Not before the engine is up. It starts on the event loop after
    /// this file first imports it -- on the first call of a run, that is
    /// now -- and until then it drops what it is told, with a warning
    /// and nothing else: the grant would be lost, and the call would open
    /// on the engine's own microphone prompt.
    function go() {
        if (!WebEngine.isInitialized()) {
            return
        }
        root.grant(root.authorityOf(root.url))
        root.target = root.url
    }

    Connections {
        target: WebEngine
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onInitialized: root.go()
    }

    onUrlChanged: root.go()
    Component.onCompleted: root.go()
    Component.onDestruction: root.revoke()

    // `active` is what WebView.qml's own binding would be -- the page on
    // screen, the app in front; the call page works that out and hands
    // it in as `shown` -- until the page has come up. A view that is
    // never made active by its page's transition draws nothing at all
    // (see WebxdcPage.qml). From then on, for as long as the call lasts,
    // it stays active: an inactive view is a hidden document, and the
    // engine pauses a hidden document's media -- which for a call is the
    // other end's voice, gone the moment the reader looks at another app.
    WebView {
        id: view
        objectName: "callView"
        anchors.fill: parent
        active: root.shown || (root.holding && view.loaded)
        url: root.target
        onUrlChanged: root.keepInside()
    }
}
