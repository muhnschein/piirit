import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.WebView 1.0
import "../components"
import Piirit 1.0

/*
 * Where an app comes from: the webxdc store, as a page.
 *
 * A file browser filtered to *.xdc was the first answer, and it is the
 * wrong one for the common case -- somebody who wants to play a game with
 * a friend has no .xdc on their phone yet and no idea where to get one.
 * deltachat-android opens the store instead, and so does this.
 *
 * The store is an ordinary website. What is not ordinary is what happens
 * when a link to an app is followed: a .xdc is a download to the engine,
 * which would save it into the browser's world where this app cannot
 * reach it -- and a download is not a navigation, so watching where the
 * view goes never hears about it at all. That was this page's first
 * answer and it did nothing on a phone.
 *
 * deltachat-android decides every navigation itself
 * (`shouldOverrideUrlLoading`) and fetches an .xdc through the core.
 * There is no such hook here, so the tap is caught where it happens: a
 * frame script in the engine's own world (qml/webxdc/catch.js) stops a
 * click on a link to an app and sends the address back over the message
 * channel, and the file is fetched through the core (`WebxdcStore`).
 * Everything else is left alone, so the store browses as a website.
 *
 * Three ways in, because only the first is certain: the frame script's
 * message, the view's own `linkClicked`, and a navigation that does
 * commit. An app is taken once however it arrives.
 *
 * Pushed by URL and reporting on `picked`, as the picker pages are and
 * for the same reason: `Sailfish.WebView` resolves only where the browser
 * engine is installed, and here that costs this page rather than the
 * conversation it was opened from.
 */
Page {
    id: page

    property int accountId

    /// An app was chosen and is on the phone at this path.
    signal picked(string path)

    /// Where the apps are. Delta Chat's own default store, which is what
    /// deltachat-android opens; it is not a setting here because there is
    /// nowhere yet to put one.
    readonly property string storeUrl: "https://webxdc.org/apps/"

    property string errorMessage: ""

    /// An app has been taken and the page is on its way out. The same tap
    /// can arrive by more than one route, and the second one is not a
    /// second app.
    property bool taken: false

    WebxdcStore {
        id: store
        objectName: "store"
        account_id: page.accountId
        onPicked: {
            page.picked(path)
            pageStack.pop()
        }
        onError: {
            // Nothing was taken after all, so the next tap is a real one.
            page.taken = false
            page.errorMessage = message
        }
    }

    /// Take the app at `where` rather than letting the engine download
    /// it.
    ///
    /// Called for every address the page hears about -- a click the frame
    /// script stopped, a link the view reports, a navigation that
    /// happened -- and it is this that decides which of them is an app.
    function catchApp(where) {
        var here = "" + where
        var path = here.split("#")[0].split("?")[0]
        if (path.slice(-4).toLowerCase() !== ".xdc"
                || page.taken || store.fetching) {
            return
        }
        page.taken = true
        // For the navigation that got as far as starting: a click the
        // frame script stopped never began one.
        view.stop()
        store.fetch(here)
    }

    /// An app already on the phone: sent by somebody, or downloaded
    /// before. The store is where one is found, not the only place one
    /// can be.
    function pickFromPhone() {
        // The empty properties are passed rather than left out: a stack
        // that declares both takes two, and the one-argument form does
        // not reach it. ProfilesPage learned the same thing about
        // replaceAbove.
        var picker = pageStack.push(Qt.resolvedUrl("AttachAppPage.qml"), {})
        if (picker) {
            picker.picked.connect(function (path) {
                page.picked(path)
                pageStack.pop(page)
            })
        }
    }

    // Not a PageHeader: the title is the store's own text. Empty until
    // the page says what it is called, which is why it has a name of its
    // own to fall back on.
    ConversationHeader {
        id: header
        objectName: "storeHeader"
        //: The heading over the webxdc app store.
        title: view.title.length > 0 ? view.title : qsTr("Apps")
    }

    WebView {
        id: view
        objectName: "storeView"
        anchors {
            top: header.bottom
            left: parent.left
            right: parent.right
            bottom: fromPhone.top
        }
        url: page.storeUrl
        // A navigation that commits. Not how an app arrives -- a .xdc is
        // downloaded rather than navigated to -- but a redirect that ends
        // on one would come this way.
        onUrlChanged: page.catchApp(view.url)
        // The view's own account of a tapped link, before the engine has
        // decided what the link is. `url` here is the signal's, not the
        // view's.
        onLinkClicked: page.catchApp(url)
    }

    // The frame script, and the message it sends back. Registered from
    // the page rather than on the view: a `Component.onCompleted` written
    // on the view would replace WebView.qml's own, which is where it
    // registers the messages it needs to work at all.
    Component.onCompleted: {
        view.addMessageListener("piirit:app")
        view.loadFrameScript(Qt.resolvedUrl("../webxdc/catch.js"))
    }

    // Connections rather than a handler on the view, for the same reason:
    // this listens beside WebView.qml's own handler instead of taking its
    // place.
    Connections {
        target: view
        onRecvAsyncMessage: {
            if (message === "piirit:app" && data) {
                page.catchApp(data.uri)
            }
        }
    }

    // Below the store rather than in a pulley: a pulley wants a
    // SilicaFlickable around it, and a flickable around a WebView takes
    // the drags the page itself needs. It is also the way out when the
    // store cannot be reached at all.
    Button {
        id: fromPhone
        objectName: "fromPhoneButton"
        anchors {
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge
            horizontalCenter: parent.horizontalCenter
        }
        //: Button under the app store: pick a .xdc file already on the phone.
        text: qsTr("From the phone")
        onClicked: page.pickFromPhone()
    }

    BusyIndicator {
        objectName: "storeBusy"
        anchors.centerIn: view
        size: BusyIndicatorSize.Large
        running: store.fetching
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
