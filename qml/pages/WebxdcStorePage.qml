import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.WebView 1.0
import "../components"
import Piirit 1.0

/*
 * Webxdc app store as page, as deltachat-android; .xdc file browser fails common case (none
 * yet). Engine would download app link into its own world, out of app's reach, and download
 * isn't navigation, so view never sees it. No `shouldOverrideUrlLoading` here: frame script
 * (qml/webxdc/catch.js) catches app-link clicks, sends address back; core fetches .xdc
 * (`WebxdcStore`). Rest browses normally. Three ways in (frame script, `linkClicked`,
 * committed navigation); app taken once. Pushed by URL: `Sailfish.WebView` resolves only
 * where engine installed.
 */
Page {
    id: page

    property int accountId

    /// App chosen, at this path.
    signal picked(string path)

    /// Store URL: Delta Chat default; no setting yet.
    readonly property string storeUrl: "https://webxdc.org/apps/"

    property string errorMessage: ""

    /// App taken; page leaving. Same tap arriving twice is one app.
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
            // Nothing taken: next tap is real.
            page.taken = false
            page.errorMessage = message
        }
    }

    /// Take app at `where` instead of engine download. Called for every address heard; decides
    /// which is an app.
    function catchApp(where) {
        var here = "" + where
        var path = here.split("#")[0].split("?")[0]
        if (path.slice(-4).toLowerCase() !== ".xdc"
                || page.taken || store.fetching) {
            return
        }
        page.taken = true
        // Stops navigation that did start (click caught by frame script never began one).
        view.stop()
        store.fetch(here)
    }

    /// App already on phone: store not only source.
    function pickFromPhone() {
        // Pass `{}`: stack declaring both args takes two; one-arg form misses it (ProfilesPage
        // hit same with replaceAbove).
        var picker = pageStack.push(Qt.resolvedUrl("AttachAppPage.qml"), {})
        if (picker) {
            picker.picked.connect(function (path) {
                page.picked(path)
                pageStack.pop(page)
            })
        }
    }

    // Not PageHeader: title is store's own; empty until page names itself, so fallback.
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
        // Committed navigation: downloads don't land here, redirects to .xdc do.
        onUrlChanged: page.catchApp(view.url)
        // Tapped link, before engine decides. `url` is signal's.
        onLinkClicked: page.catchApp(url)
    }

    // Frame script, reply. Here: view's `Component.onCompleted` would replace WebView.qml's.
    Component.onCompleted: {
        view.addMessageListener("piirit:app")
        view.loadFrameScript(Qt.resolvedUrl("../webxdc/catch.js"))
    }

    // Connections: listens beside WebView.qml's handler.
    Connections {
        target: view
        onRecvAsyncMessage: {
            if (message === "piirit:app" && data) {
                page.catchApp(data.uri)
            }
        }
    }

    // Below store: pulley wants SilicaFlickable, taking page's drags. Also way out when store
    // unreachable.
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
