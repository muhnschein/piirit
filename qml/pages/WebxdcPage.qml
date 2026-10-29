import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.WebView 1.0
import "../components"
import Piirit 1.0

/*
 * Running webxdc app: zip files served via core on loopback, API injected
 * (`webxdc_host.rs`). Pushed by URL: `Sailfish.WebView` resolves only where engine
 * installed. Leaving destroys the serving object, closing the socket.
 */
Page {
    id: page

    // Core move held until page on screen; a call page may cover it (PendingNavigation.qml).
    PendingNavigation {
        id: navigation
        stack: pageStack
        ready: page.status === PageStatus.Active
    }

    property int accountId
    property int messageId

    /// App's name from the row, until core answers.
    property string appName: ""

    /// Heading instead of app's name, and line under it (map: chat's name, "Map").
    property string heading: ""
    property string subheading: ""

    /// Appended to app's address, e.g. map's `?poi=…`.
    property string query: ""

    /// Where view goes: app's address plus `query`; empty until host answers.
    readonly property string address: app.url.length > 0 ? app.url + page.query : ""

    property string errorMessage: ""

    /// App drew once; later loads bring no spinner.
    property bool drew: false

    /// Served `127.0.0.1:port`, which app may not leave.
    function authorityOf(where) {
        var at = ("" + where).indexOf("//")
        if (at < 0) {
            return ""
        }
        var rest = ("" + where).substring(at + 2)
        var slash = rest.indexOf("/")
        return slash < 0 ? rest : rest.substring(0, slash)
    }

    /// Put view back if app navigates off-host (policy blocks network, but page could reach
    /// open web under app's name). Address only: else engine's `about:blank`/`about:neterror`
    /// get replaced by blank.
    function keepInside() {
        var here = page.authorityOf(view.url)
        var ours = page.authorityOf(app.url)
        if (ours.length === 0 || here.length === 0 || here === ours) {
            return
        }
        view.stop()
        // App broke the binding; restore binding, not a fixed address: close still empties view.
        view.url = Qt.binding(function () { return page.address })
    }

    WebxdcApp {
        id: app
        objectName: "app"
        account_id: page.accountId
        message_id: page.messageId
        onError: page.errorMessage = message
        // Message deleted (any device): nothing to run.
        onGone: navigation.pop()
        // Handed-over file kept, not sent to a chat (see `offer`).
        onHanded_over: page.offer(file_path, text)
    }

    /// Save app's file unasked (its button is a download) via AttachmentSaver, with notice.
    /// Text alone goes to clipboard, with notice: never dropped.
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

    /// Path as URL, encoded per segment: "#" or "%" in name would misdirect.
    function urlOf(filePath) {
        return Qt.resolvedUrl("file://" + filePath.split("/")
                                                  .map(encodeURIComponent)
                                                  .join("/"))
    }

    // Same copy and folder as conversation attachments.
    AttachmentSaver {
        id: handoverSaver
        objectName: "handoverSaver"
        /// Cache copies awaiting save, oldest first (off-thread copy answers in order).
        /// Dropped once saved or failed: no retry, no leftovers.
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
        // Core not up: nothing to serve yet; wait, as chat list does.
        onStatus_changed: page.run()
    }

    function run() {
        if (core.status === "ready") {
            app.reload()
            app.start()
        }
    }

    Component.onCompleted: page.run()

    // Leaving (destruction: pop or replace) stops app; covering doesn't. Not `Deactivating`:
    // `sendToChat` picker over app would kill host mid-request.
    Component.onDestruction: app.stop()

    // Not PageHeader: can't draw app's name as plain text. No pulley: SilicaFlickable takes
    // app's drags. `source_code_url` on shim (docs/PROJECT.md).
    ConversationHeader {
        id: header
        objectName: "webxdcHeader"
        title: page.heading.length > 0 ? page.heading
               : app.name.length > 0 ? app.name : page.appName
        subtitle: page.subheading
    }

    // Empty until host gives address. `active` left to WebView.qml (page status, focus):
    // overriding left grey, never-activated view. Address only as binding: assigned drew grey.
    WebView {
        id: view
        objectName: "webxdcView"
        anchors {
            top: header.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        url: page.address
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

    // Shown until app draws, with reason if it never does.
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
        // Reason stays here: banner clears in seconds, leaving blank view unexplained.
        //: Shown while a webxdc app is being made ready to run.
        text: page.errorMessage.length > 0 ? page.errorMessage
                                           : qsTr("Starting the app")
    }

    // Stateless notice: file copied out, or text on clipboard.
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
