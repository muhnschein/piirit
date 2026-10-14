import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.WebView 1.0
import "../components"
import Piirit 1.0

/*
 * A webxdc app, running. App is a served zip, not unpacked: shim serves each file
 * from the archive via core on a loopback address; WebView points there (`webxdc_host.rs`).
 * API injected by host.
 *
 * Own page, pushed by URL: `Sailfish.WebView` resolves only where engine installed.
 *
 * Object serves the app: leaving the page destroys it and closes the socket.
 */
Page {
    id: page

    // Move from core, held until this page is on screen: a call's page may cover it
    // (PendingNavigation.qml).
    PendingNavigation {
        id: navigation
        stack: pageStack
        ready: page.status === PageStatus.Active
    }

    property int accountId
    property int messageId

    /// App's name from the row, until core answers.
    property string appName: ""

    property string errorMessage: ""

    /// App has drawn once. Waits only once: later loads don't bring back the spinner.
    property bool drew: false

    /// Served address, `127.0.0.1:port`. What the app may not leave.
    function authorityOf(where) {
        var at = ("" + where).indexOf("//")
        if (at < 0) {
            return ""
        }
        var rest = ("" + where).substring(at + 2)
        var slash = rest.indexOf("/")
        return slash < 0 ? rest : rest.substring(0, slash)
    }

    /// Put view back if app moves itself to another address.
    ///
    /// No network by policy, but page could move to open web under app's name.
    /// Compare address only: engine's
    /// `about:blank`/`about:neterror` pages would otherwise be replaced by blank page.
    function keepInside() {
        var here = page.authorityOf(view.url)
        var ours = page.authorityOf(app.url)
        if (ours.length === 0 || here.length === 0 || here === ours) {
            return
        }
        view.stop()
        // Rewrite binding: app overwrote it; binding re-runs only on `app.url` change.
        // Restoring binding (not address) keeps view following app, so close still empties it.
        view.url = Qt.binding(function () { return app.url })
    }

    WebxdcApp {
        id: app
        objectName: "app"
        account_id: page.accountId
        message_id: page.messageId
        onError: page.errorMessage = message
        // Message deleted here or on another device: nothing left to run.
        onGone: navigation.pop()
        // App handed a file over: kept, not put in a chat (see `offer`).
        onHanded_over: page.offer(file_path, text)
    }

    /// Keep a file the app produced.
    ///
    /// Saved, not asked (the app's button is a download). Copy goes where every copy
    /// goes (AttachmentSaver), with notice. Text without a file goes to clipboard,
    /// also with notice: never dropped silently.
    function offer(filePath, text) {
        if (filePath.length === 0) {
            if (text.length > 0) {
                Clipboard.text = text
                notice.show(qsTr("Copied to clipboard"))
            }
            return
        }
        handoverSaver.handedOver = handoverSaver.handedOver.concat([filePath])
        handoverSaver.keep(page.urlOf(filePath), "")
    }

    /// Path as URL, encoded: a "#" or "%" in app's name would point elsewhere. Per segment.
    function urlOf(filePath) {
        return Qt.resolvedUrl("file://" + filePath.split("/")
                                                  .map(encodeURIComponent)
                                                  .join("/"))
    }

    // Saving: same copy the conversation makes of an attachment, same folder.
    AttachmentSaver {
        id: handoverSaver
        objectName: "handoverSaver"
        /// File app handed over: cache copy awaiting save. Dropped once saved or
        /// failed (no retry; no pile of leftovers).
        ///
        /// List, oldest first: copy is off the Qt thread, so answers keep request order.
        property var handedOver: []
        onSaved: {
            notice.show(handoverSaver.savedText)
            handoverSaver.forget()
        }
        onError: {
            page.errorMessage = message
            handoverSaver.forget()
        }
        function forget() {
            var waiting = handoverSaver.handedOver
            if (waiting.length > 0) {
                app.discard(waiting[0])
                handoverSaver.handedOver = waiting.slice(1)
            }
        }
    }

    Connections {
        target: core
        onCore_event: app.handle_event(context_id, kind, payload_json)
        // App started before core up has nothing to serve: page waits, as chat list does.
        onStatus_changed: page.run()
    }

    function run() {
        if (core.status === "ready") {
            app.reload()
            app.start()
        }
    }

    Component.onCompleted: page.run()

    // Leaving stops app; covering does not. Not `Deactivating`: `sendToChat` opens picker over app,
    // stopping there would kill host mid-request. Destruction is the leave signal (pop or replace).
    Component.onDestruction: app.stop()

    // Not a PageHeader: name is the app's own, and header can't draw plain text.
    // No pulley: needs SilicaFlickable, which takes the app's drags. `source_code_url` is on shim
    // (docs/PROJECT.md).
    ConversationHeader {
        id: header
        objectName: "webxdcHeader"
        title: app.name.length > 0 ? app.name : page.appName
    }

    // Where app draws. Nothing loads until host answers with an address; empty URL shows nothing.
    //
    // `active` not set here: WebView.qml binds it to page status and app focus. Overriding it left
    // a grey view never activated by the transition.
    //
    // Address is a binding, never assigned: a handler-set address drew grey; `url:` binding worked.
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

    // What shows until app draws something, and why if it never does. Gone once app is up.
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
        // Reason, not the wait: banner clears itself after seconds, leaving blank view unexplained.
        //: Shown while a webxdc app is being made ready to run.
        text: page.errorMessage.length > 0 ? page.errorMessage
                                           : qsTr("Starting the app")
    }

    // Says what just happened where there's no state: file copied out, or text on clipboard.
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
