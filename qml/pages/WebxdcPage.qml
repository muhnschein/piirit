import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.WebView 1.0
import "../components"
import Piirit 1.0

/*
 * A webxdc app, running.
 *
 * The app is a zip somebody sent, and it is *served* rather than
 * unpacked: the shim puts it on a loopback address of its own, reading
 * every file out of the archive through the core, and this points a
 * WebView at that address (`webxdc_host.rs`). The API the app talks to
 * the chat through is injected by the same host, so nothing here has to
 * reach into the page.
 *
 * A page of its own, pushed by URL, for the reason the picker pages are:
 * `Sailfish.WebView` is the one type in this tree that resolves only on a
 * release that ships the browser engine, and here that costs this page
 * rather than the conversation it was opened from.
 *
 * The object is what serves the app, so leaving the page stops it: the
 * page is destroyed on the way out, the object with it, and the socket
 * closes. Nothing has to be remembered to be cleaned up later.
 */
Page {
    id: page

    // A move the core asked for, held until this page is the one on
    // screen: a call's page may be over it, and a move made then would
    // take that page instead (PendingNavigation.qml).
    PendingNavigation {
        id: navigation
        stack: pageStack
        ready: page.status === PageStatus.Active
    }

    property int accountId
    property int messageId

    /// What the row said the app was called, until the core answers.
    property string appName: ""

    property string errorMessage: ""

    /// The app has drawn at least once. What the page waits for, and it
    /// only waits once: an app that loads something of its own later
    /// should not have this page's own spinner put back over the top of
    /// it. The view carries one of its own for that.
    property bool drew: false

    /// The address the app is served on, `127.0.0.1:port`. What the app
    /// may not leave.
    function authorityOf(where) {
        var at = ("" + where).indexOf("//")
        if (at < 0) {
            return ""
        }
        var rest = ("" + where).substring(at + 2)
        var slash = rest.indexOf("/")
        return slash < 0 ? rest : rest.substring(0, slash)
    }

    /// Put the view back if the app takes itself to another address.
    ///
    /// A webxdc has no network -- the policy it is served under permits
    /// its own origin and nothing else -- but a policy does not stop the
    /// page itself moving, and an app that set `window.location` to the
    /// open web would be showing it inside a page wearing that app's
    /// name. So: it stays where it was put.
    ///
    /// The address is what is compared, not the whole URL. The engine's
    /// own pages have no address at all -- `about:blank` is where a view
    /// starts and `about:neterror` is how it says a load failed -- and
    /// turning those back would replace the reason with a blank page and
    /// keep doing it. Anything the engine does to the URL short of
    /// changing where it points is likewise none of this function's
    /// business.
    function keepInside() {
        var here = page.authorityOf(view.url)
        var ours = page.authorityOf(app.url)
        if (ours.length === 0 || here.length === 0 || here === ours) {
            return
        }
        view.stop()
        // The binding below, written again: an app that moved the view
        // wrote over it, and a binding only re-runs when what it reads
        // changes -- which `app.url` has not. Putting the binding back
        // rather than the address means the view still follows the app
        // afterwards, so closing the page still empties it.
        view.url = Qt.binding(function () { return app.url })
    }

    WebxdcApp {
        id: app
        objectName: "app"
        account_id: page.accountId
        message_id: page.messageId
        onError: page.errorMessage = message
        // The message is gone -- deleted here or on another device --
        // so there is nothing left to run.
        onGone: navigation.pop()
        // The app has handed a file over. It is kept, not put into a
        // chat: see `offer` below.
        onHanded_over: page.offer(file_path, text)
    }

    /// Keep a file the app produced.
    ///
    /// Saved rather than asked about. The button an app draws over this
    /// is a download, and a download does not ask -- a chooser between
    /// opening and keeping is two taps in front of the one thing the
    /// reader has already asked for. The copy goes where every copy
    /// goes (AttachmentSaver), and the notice says so.
    ///
    /// Text with no file has nowhere to be saved, so it goes on the
    /// clipboard instead and the page says that too: the reader asked
    /// for it either way, and dropping it silently is the one thing that
    /// would not be an answer.
    function offer(filePath, text) {
        if (filePath.length === 0) {
            if (text.length > 0) {
                Clipboard.text = text
                notice.show(qsTr("Copied to clipboard"))
            }
            return
        }
        handoverSaver.handedOver = filePath
        handoverSaver.keep(page.urlOf(filePath), "")
    }

    /// A path as a URL, encoded rather than concatenated: the name is the
    /// app's, and a "#" or a "%" in one makes a plain "file://" + path
    /// point somewhere else. Per segment, as AttachmentPreview does it.
    function urlOf(filePath) {
        return Qt.resolvedUrl("file://" + filePath.split("/")
                                                  .map(encodeURIComponent)
                                                  .join("/"))
    }

    // Saving is the same copy the conversation makes of an attachment,
    // into the same folder.
    AttachmentSaver {
        id: handoverSaver
        objectName: "handoverSaver"
        /// The file the app handed over: a copy in the cache, waiting to
        /// be saved. Once the reader has their own copy the cached one is
        /// dead weight, so it goes either way -- a save that failed will
        /// not be retried from it, and an app that hands over a hundred
        /// files should not leave a hundred behind.
        property string handedOver
        onSaved: {
            notice.show(handoverSaver.savedText)
            handoverSaver.forget()
        }
        onError: {
            page.errorMessage = message
            handoverSaver.forget()
        }
        function forget() {
            if (handoverSaver.handedOver.length > 0) {
                app.discard(handoverSaver.handedOver)
                handoverSaver.handedOver = ""
            }
        }
    }

    Connections {
        target: core
        onCore_event: app.handle_event(context_id, kind, payload_json)
        // An app started before the core was up has nothing to be served
        // from; the page opens as the chat list does and waits.
        onStatus_changed: page.run()
    }

    function run() {
        if (core.status === "ready") {
            app.reload()
            app.start()
        }
    }

    Component.onCompleted: page.run()

    // Leaving stops the app; being covered by another page does not.
    //
    // Not `Deactivating`, which fires for both: `sendToChat` opens the
    // chat picker *over* the app at the app's own request, so stopping
    // there would kill the host in the middle of the request that asked,
    // and the app's fetch would be answered by a closed socket.
    //
    // Destruction is the honest signal for leaving: a popped page is
    // destroyed, and a stack that is replaced takes the page with it.
    // Both reach here, and both stop the app. What stays running is an
    // app the reader is coming back to.
    Component.onDestruction: app.stop()

    // Not a PageHeader: the name is the app's own, and a header cannot be
    // told to draw what it is given as plain text.
    //
    // Nothing is offered on it. A pulley is the usual home for "where did
    // this come from", and there is nowhere here to put one: Silica's
    // pulley wants a SilicaFlickable around it, and a flickable around a
    // WebView takes the drags the app itself needs. The app's
    // `source_code_url` is on the shim object either way, for whatever
    // shows it (docs/PROJECT.md).
    ConversationHeader {
        id: header
        objectName: "webxdcHeader"
        title: app.name.length > 0 ? app.name : page.appName
    }

    // Where the app draws. Nothing is loaded until the host answers with
    // an address, and an empty URL is a WebView showing nothing rather
    // than a blank page from somewhere else.
    //
    // `active` is deliberately not set here. WebView.qml binds it to the
    // page's own status and to whether the app is in front, which is what
    // decides when the engine renders and when it lets go of the GPU;
    // overriding it left a view that was never activated by the page
    // transition -- a grey rectangle where the app should be.
    //
    // The address is a binding, and nothing assigns it on the way in.
    // The first version pointed the view from a handler on the shim's
    // signal and drew grey on a phone where the store page -- the same
    // type, a plain `url:` binding -- drew a website; whatever the engine
    // makes of the difference, this is the shape that works.
    WebView {
        id: view
        objectName: "webxdcView"
        anchors {
            top: header.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        url: app.url
        onUrlChanged: page.keepInside()
        onLoadedChanged: {
            if (view.loaded) {
                page.drew = true
            }
        }
    }

    BusyIndicator {
        objectName: "webxdcBusy"
        anchors.centerIn: view
        size: BusyIndicatorSize.Large
        running: !page.drew && page.errorMessage.length === 0
    }

    // What is happening until the app has drawn something, and why when
    // it will not. Gone the moment the app is up.
    Label {
        objectName: "webxdcWaiting"
        anchors {
            top: parent.verticalCenter
            topMargin: Theme.paddingLarge
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        visible: !page.drew
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: Text.PlainText
        // The reason, when there is one, rather than the wait: the banner
        // says it too and then clears itself after a few seconds, which
        // on a view that never drew anything leaves the reader looking at
        // the same grey rectangle as before with nothing to read.
        //: Shown while a webxdc app is being made ready to run.
        text: page.errorMessage.length > 0 ? page.errorMessage
                                           : qsTr("Starting the app")
    }

    // Says what just happened where the page has no state for it: a file
    // copied out, or a line of text put on the clipboard.
    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 4
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        text: page.errorMessage
        onDismissed: page.errorMessage = ""
    }
}
