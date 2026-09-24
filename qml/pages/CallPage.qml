import QtQuick 2.0
import Sailfish.Silica 1.0
import QtSensors 5.0
import "../components"

/*
 * The one call the app is in: ringing, being set up, in progress, and
 * over.
 *
 * The call itself is the window's (`CallCenter.qml`), not this page's --
 * a call comes in whatever is on screen -- and this is where it is shown.
 * Who is at the other end and how the call stands are drawn here, in
 * the phone's language; while it rings, answering and declining are
 * here too. Once answered, the rest is the call's own page, running in
 * the browser engine (`CallView.qml`), which carries the controls a call
 * needs while it lasts: the red button and the microphone.
 *
 * That page is loaded rather than declared, so a phone without the
 * browser engine loses the call and keeps this page -- a call that rings
 * there can still be seen, and declined.
 *
 * The page cannot be swiped away while the call is up. Leaving it would
 * take the call's page with it, and the call with that: a call the
 * reader has not hung up that has stopped carrying sound. Hanging up is
 * how it is left, and the page goes by itself once the call is over.
 */
Page {
    id: page

    /// The window's call.
    property QtObject call: null

    /// A call is under way: anything short of over.
    readonly property bool busy: page.call !== null && page.call.state !== ""
                                 && page.call.state !== "ended"
    /// The two ends have agreed to talk: the call's page is where it
    /// goes on.
    readonly property bool inCall: page.busy && page.call.state !== "ringing"

    /// Now, moved on once a second while the clock is shown.
    property real now: Date.now()

    /// What the cover's quick actions must not jump away from.
    readonly property bool pausesQuickActions: page.busy

    backNavigation: !page.busy

    /// How long the call has been connected, as the phone app shows it.
    function elapsed() {
        if (!page.call || page.call.connected_at <= 0) {
            return ""
        }
        var seconds = Math.max(0, Math.floor(page.now / 1000 - page.call.connected_at))
        var minutes = Math.floor(seconds / 60)
        var rest = seconds % 60
        return minutes + ":" + (rest < 10 ? "0" : "") + rest
    }

    /// How the call stands, in words. Delta Chat's own wherever it has
    /// them, so its translations apply. Not `status`, which is the
    /// page's own.
    function describe() {
        if (!page.call) {
            return ""
        }
        var state = page.call.state
        if (state === "ringing") {
            return qsTr("Incoming call")
        }
        if (state === "calling") {
            return qsTr("Ringing…")
        }
        if (state === "starting" || state === "connecting") {
            return qsTr("Connecting…")
        }
        if (state === "reconnecting") {
            return qsTr("Reconnecting…")
        }
        if (state === "connected") {
            return page.elapsed()
        }
        if (state === "ended") {
            var reason = page.call.end_reason
            if (reason === "answered-elsewhere") {
                return qsTr("Call answered on another device")
            }
            if (reason === "failed") {
                return qsTr("Call failed")
            }
            if (reason === "missed") {
                return qsTr("Missed call")
            }
            if (reason === "declined") {
                return qsTr("Declined call")
            }
            return qsTr("Call ended")
        }
        return ""
    }

    Timer {
        objectName: "clock"
        interval: 1000
        repeat: true
        running: page.call !== null && page.call.state === "connected"
        triggeredOnStart: true
        onTriggered: page.now = Date.now()
    }

    // Over: said for a moment, then the page goes. From the page on
    // screen only -- a permission prompt still over it goes first -- so
    // the pop takes this page and nothing the reader is looking at.
    property bool closeWhenActive: false

    Timer {
        id: leave
        objectName: "leave"
        interval: 1500
        onTriggered: page.close()
    }

    function close() {
        if (page.busy) {
            return
        }
        if (page.status !== PageStatus.Active) {
            page.closeWhenActive = true
            return
        }
        page.closeWhenActive = false
        if (page.call && page.call.state === "ended") {
            page.call.reset()
        }
        pageStack.pop()
    }

    onStatusChanged: {
        if (page.status === PageStatus.Active && page.closeWhenActive) {
            page.close()
        }
    }

    // A call that comes in while the last one is still being said to be
    // over keeps the page: it is this call's now.
    onBusyChanged: {
        if (page.busy) {
            leave.stop()
            page.closeWhenActive = false
        }
    }

    Connections {
        target: page.call
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onEnded: leave.restart()
    }

    // A page that goes while its call is still up -- the stack replaced
    // under it -- takes the call's media with it. Hanging up then is the
    // honest end: the other side stops talking to nobody.
    Component.onDestruction: {
        if (page.busy) {
            page.call.hang_up()
        }
    }

    // Who, and how it stands. Over the call's page rather than inside
    // it: the page draws its own words in English only.
    Column {
        id: who
        objectName: "callWho"
        anchors {
            top: parent.top
            topMargin: page.inCall && view.status === Loader.Ready
                       ? Theme.paddingLarge : Theme.itemSizeExtraLarge
        }
        width: parent.width
        spacing: Theme.paddingMedium

        Avatar {
            objectName: "callAvatar"
            anchors.horizontalCenter: parent.horizontalCenter
            // Small once the call's page is up and draws its own.
            visible: !(page.inCall && view.status === Loader.Ready)
            width: 2 * Theme.itemSizeExtraLarge
            initial: page.call ? page.call.peer_name : ""
            ownColor: page.call ? page.call.peer_color : ""
            picturePath: page.call ? page.call.peer_avatar : ""
        }

        Label {
            objectName: "callName"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            horizontalAlignment: Text.AlignHCenter
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeExtraLarge
            font.family: Theme.fontFamilyHeading
            color: Theme.highlightColor
            // The other end's own name for themselves.
            textFormat: Text.PlainText
            text: page.call ? page.call.peer_name : ""
        }

        Label {
            objectName: "callStatus"
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            color: Theme.secondaryHighlightColor
            textFormat: Text.PlainText
            text: page.describe()
        }
    }

    // The call's page, once there is one to show.
    Loader {
        id: view
        objectName: "callViewLoader"
        anchors {
            top: who.bottom
            topMargin: Theme.paddingLarge
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        active: page.call !== null && page.call.url.length > 0
        source: Qt.resolvedUrl("../components/CallView.qml")
        onLoaded: {
            view.item.holding = Qt.binding(function () { return page.busy })
            // What WebView.qml would bind `active` to itself.
            view.item.shown = Qt.binding(function () {
                return Qt.application.state === Qt.ApplicationActive
                        && (page.status === PageStatus.Active
                            || page.status === PageStatus.Deactivating)
            })
            view.item.url = Qt.binding(function () {
                return page.call ? page.call.url : ""
            })
        }
    }

    // Answering and declining, while it rings. Declining is on the left
    // and answering on the right, where the phone's own call screen has
    // them.
    Row {
        objectName: "ringingButtons"
        visible: page.call !== null && page.call.state === "ringing"
        anchors {
            bottom: parent.bottom
            bottomMargin: Theme.itemSizeLarge
            horizontalCenter: parent.horizontalCenter
        }
        spacing: Theme.paddingLarge * 2

        Button {
            objectName: "declineButton"
            //: Declines a call that is ringing. A verb.
            text: qsTr("Decline")
            color: Theme.errorColor
            onClicked: page.call.hang_up()
        }

        Button {
            objectName: "answerButton"
            //: Answers a call that is ringing. A verb.
            text: qsTr("Answer")
            onClicked: page.call.answer()
        }
    }

    // Hanging up while there is no call page to do it from: before it
    // has come up, and on a phone where it cannot.
    Button {
        objectName: "hangUpButton"
        visible: page.inCall && view.status !== Loader.Ready
        anchors {
            bottom: parent.bottom
            bottomMargin: Theme.itemSizeLarge
            horizontalCenter: parent.horizontalCenter
        }
        color: Theme.errorColor
        //: Ends a call in progress.
        text: qsTr("End call")
        onClicked: page.call.hang_up()
    }

    // Held to the ear, the screen is a cheek's to press. The phone app
    // has the display switched off for it; an app cannot, so what is
    // under the cheek is black and takes no touch. Only for a call the
    // phone is held to: while it rings the reader is looking at it.
    ProximitySensor {
        id: proximity
        objectName: "proximity"
        active: page.inCall && Qt.application.state === Qt.ApplicationActive
    }

    Rectangle {
        objectName: "earShield"
        anchors.fill: parent
        z: 10
        color: "black"
        visible: proximity.active && proximity.reading !== null
                 && proximity.reading.near === true

        MouseArea {
            anchors.fill: parent
        }
    }
}
