import QtQuick 2.0
import Sailfish.Silica 1.0
import QtSensors 5.0
import QtGraphicalEffects 1.0
import "../components"

/*
 * The one call the app is in: ringing, being set up, in progress, and
 * over.
 *
 * The call itself is the window's (`CallCenter.qml`), not this page's --
 * a call comes in whatever is on screen -- and this is where it is shown.
 * Laid out as the phone's own call screen lays out a call. Ringing: who,
 * large and centred at the top, and the handset in the middle, to drag
 * to either side to answer or up to silence the ringing -- after which
 * the call can be declined, declined with a message, or declined with a
 * reminder to call back. In a call: who, at the top on the left, and the
 * time on the right; how long, large, under them; the loudspeaker and
 * the microphone under that; and End call at the foot. Their picture,
 * where they have one of their own, fills the screen behind it all, out
 * of focus.
 *
 * The media runs in the browser engine, in upstream's call page
 * (`CallView.qml`), which draws controls of its own. They are not shown:
 * the page is kept running unseen, and what its controls did is done
 * from here -- the microphone through the call, which has the page press
 * its own switch, and hanging up through the call as it always was.
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

    /// The window's call handling (CallCenter.qml): where the call's
    /// sound goes, and what a ringing call can be silenced into.
    property QtObject center: null

    /// The call is ringing here, and the screen is the one a phone rings
    /// with.
    readonly property bool ringing: page.call !== null && page.call.state === "ringing"

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

    /// The call is connected, and its clock is running.
    readonly property bool connected: page.call !== null && page.call.state === "connected"

    /// How long the call has been connected, as the phone app shows it:
    /// hours, minutes and seconds, two digits each.
    function elapsed() {
        if (!page.call || page.call.connected_at <= 0) {
            return ""
        }
        var seconds = Math.max(0, Math.floor(page.now / 1000 - page.call.connected_at))
        var parts = [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60]
        return parts.map(function (part) { return (part < 10 ? "0" : "") + part }).join(":")
    }

    /// The same, with the groups that are still nothing but zeros drawn
    /// quieter, as the phone app draws them: what is counting stands out.
    function elapsedMarkup() {
        var text = page.elapsed()
        var lead = /^(00:){0,2}/.exec(text)[0]
        if (lead.length === 0) {
            return text
        }
        return "<font color=\"" + Theme.secondaryHighlightColor + "\">" + lead + "</font>"
                + text.substring(lead.length)
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

    // For the call's clock and the time of day, both.
    Timer {
        objectName: "clock"
        interval: 1000
        repeat: true
        running: page.call !== null && page.call.state !== ""
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

    // Their picture, when they have one of their own: the whole screen,
    // out of focus, and dimmed under what is drawn over it. Nothing where
    // they have none, and the ambience shows through as on every page.
    Item {
        id: backdrop
        objectName: "callBackdrop"
        anchors.fill: parent
        visible: picture.status === Image.Ready

        Image {
            id: picture
            anchors.fill: parent
            visible: false
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            // Small: a blur this wide keeps nothing a larger one would.
            sourceSize.width: 256
            sourceSize.height: 256
            // Encoded per segment; see AttachmentPreview.qml's fileUrl.
            source: page.call && page.call.peer_avatar.length > 0
                    ? Qt.resolvedUrl("file://" + page.call.peer_avatar.split("/")
                                                     .map(encodeURIComponent).join("/"))
                    : ""
        }

        FastBlur {
            anchors.fill: parent
            source: picture
            radius: 64
        }

        Rectangle {
            anchors.fill: parent
            color: Theme.rgba(Theme.highlightDimmerColor, 0.6)
        }
    }

    // The call's page: the media, and nothing on screen. Its own controls
    // are this page's to draw, so it runs unseen -- running, not hidden,
    // which the engine would pause -- under what is drawn here.
    Loader {
        id: view
        objectName: "callViewLoader"
        anchors.fill: parent
        opacity: 0
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

    // Unseen is not untouchable: a tap anywhere the page's own buttons
    // are not would land on one of that page's, drawn or not. Taken here.
    MouseArea {
        objectName: "callViewShield"
        anchors.fill: parent
    }

    // Who, on the left, and the time of day, on the right: the line the
    // phone's own call screen opens with.
    Item {
        id: who
        objectName: "callWho"
        visible: !page.ringing
        anchors {
            top: parent.top
            topMargin: 2 * Theme.paddingLarge
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        height: name.height

        Label {
            id: name
            objectName: "callName"
            width: parent.width - clock.width - Theme.paddingLarge
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.primaryColor
            // The other end's own name for themselves.
            textFormat: Text.PlainText
            text: page.call ? page.call.peer_name : ""
        }

        Label {
            id: clock
            objectName: "callClock"
            anchors.right: parent.right
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.secondaryColor
            textFormat: Text.PlainText
            // The same form as a message's time.
            text: Qt.formatTime(new Date(page.now), "hh:mm")
        }
    }

    // How long, once the two can hear each other; how it stands until
    // then, and once it is over. On a line as tall as the clock's
    // whatever it says, so the switches under it stay where they are.
    Item {
        id: statusLine
        visible: !page.ringing
        anchors {
            top: who.bottom
            topMargin: Theme.itemSizeLarge
        }
        width: parent.width
        height: clockMetric.height

        // One line of the clock's font, measured.
        Label {
            id: clockMetric
            visible: false
            font.family: Theme.fontFamilyHeading
            font.pixelSize: Theme.fontSizeHuge
            text: "00:00:00"
        }

        Label {
            objectName: "callStatus"
            anchors.verticalCenter: parent.verticalCenter
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            font.family: Theme.fontFamilyHeading
            font.pixelSize: page.connected ? Theme.fontSizeHuge : Theme.fontSizeLarge
            color: Theme.highlightColor
            // Markup only for the clock, which is made here of digits.
            textFormat: page.connected ? Text.StyledText : Text.PlainText
            text: page.connected ? page.elapsedMarkup() : page.describe()
        }
    }

    // The switches, under the clock: the loudspeaker and the
    // microphone. The phone's own screen also has the keypad and
    // recording, which belong to a phone call. As far apart as the
    // phone's four are.
    Switch {
        id: loud
        objectName: "speakerSwitch"
        visible: page.inCall
        anchors {
            top: statusLine.bottom
            topMargin: 2 * Theme.paddingLarge
            horizontalCenter: parent.horizontalCenter
            horizontalCenterOffset: -page.width / 12
        }
        icon.source: "image://theme/icon-m-speaker"
        automaticCheck: false
        checked: page.center !== null && page.center.speaker
        onClicked: page.center.speaker = !page.center.speaker
    }

    Switch {
        id: mute
        objectName: "muteSwitch"
        visible: page.inCall
        anchors {
            top: statusLine.bottom
            topMargin: 2 * Theme.paddingLarge
            horizontalCenter: parent.horizontalCenter
            horizontalCenterOffset: page.width / 12
        }
        icon.source: "image://theme/icon-m-mic-mute"
        automaticCheck: false
        checked: page.call !== null && page.call.muted
        onClicked: page.call.mute(!page.call.muted)
    }

    // Ringing: the phone's own screen for it.
    Item {
        id: ringingView
        objectName: "ringingView"
        anchors.fill: parent
        visible: page.ringing

        /// The ringing has been silenced, and what can be done instead of
        /// answering is on offer.
        readonly property bool silenced: page.center !== null && page.center.quiet

        // Who, centred and large, and what is calling.
        Label {
            id: ringName
            objectName: "ringName"
            anchors {
                top: parent.top
                topMargin: Theme.itemSizeLarge
            }
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            horizontalAlignment: Text.AlignHCenter
            truncationMode: TruncationMode.Fade
            font.family: Theme.fontFamilyHeading
            font.pixelSize: Theme.fontSizeExtraLarge
            color: Theme.primaryColor
            // The other end's own name for themselves.
            textFormat: Text.PlainText
            text: page.call ? page.call.peer_name : ""
        }

        Label {
            objectName: "ringSource"
            anchors {
                top: ringName.bottom
                horizontalCenter: parent.horizontalCenter
            }
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.primaryColor
            textFormat: Text.PlainText
            // Where the phone's own says what kind of number it is.
            text: "Piirit • " + qsTr("Audio call")
        }

        // Lower down while it rings; higher once silenced, to make room
        // for what comes instead.
        CallSwipe {
            id: swipe
            objectName: "callSwipe"
            y: (ringingView.silenced ? 0.40 : 0.62) * parent.height - height / 2
            silenced: ringingView.silenced
            onAnswered: page.call.answer()
            onSilenceRequested: page.center.silence()

            Behavior on y {
                NumberAnimation { duration: 250; easing.type: Easing.InOutQuad }
            }
        }

        Label {
            objectName: "silencedLabel"
            visible: ringingView.silenced
            anchors.horizontalCenter: parent.horizontalCenter
            y: 0.69 * parent.height - height / 2
            color: Theme.primaryColor
            textFormat: Text.PlainText
            //: A ringing call whose ringtone has been stopped.
            text: qsTr("Call silenced")
        }

        // What a silenced call can be instead: a message, declined, or a
        // reminder. Laid out by bindings, a third of the width each.
        Item {
            id: options
            objectName: "ringingOptions"
            visible: ringingView.silenced
            y: 0.73 * parent.height
            width: parent.width
            height: Theme.itemSizeLarge

            Repeater {
                model: [
                    //: Declines a ringing call and opens its chat, to write instead.
                    { "name": "message", "icon": "icon-m-message", "label": qsTr("Message") },
                    //: Declines a call that is ringing. A verb.
                    { "name": "decline", "icon": "icon-m-call", "label": qsTr("Decline") },
                    //: Declines a ringing call, and reminds the reader to call back later.
                    { "name": "remind", "icon": "icon-m-alarm", "label": qsTr("Remind me") }
                ]

                BackgroundItem {
                    id: option
                    objectName: modelData.name + "Option"
                    x: Theme.horizontalPageMargin
                       + index * (options.width - 2 * Theme.horizontalPageMargin) / 3
                    width: (options.width - 2 * Theme.horizontalPageMargin) / 3
                    height: options.height
                    onClicked: page.instead(modelData.name)

                    readonly property bool declines: modelData.name === "decline"

                    // Declining is the handset hung up, in red.
                    Image {
                        id: optionIcon
                        anchors {
                            top: parent.top
                            topMargin: Theme.paddingMedium
                            horizontalCenter: parent.horizontalCenter
                        }
                        width: Theme.iconSizeMedium
                        height: Theme.iconSizeMedium
                        rotation: option.declines ? 135 : 0
                        source: "image://theme/" + modelData.icon + "?"
                                + (option.declines ? Theme.errorColor
                                   : option.highlighted ? Theme.highlightColor
                                                        : Theme.primaryColor)
                    }

                    Label {
                        anchors {
                            top: optionIcon.bottom
                            horizontalCenter: parent.horizontalCenter
                        }
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        truncationMode: TruncationMode.Fade
                        font.pixelSize: Theme.fontSizeSmall
                        color: option.highlighted ? Theme.highlightColor : Theme.primaryColor
                        textFormat: Text.PlainText
                        text: modelData.label
                    }
                }
            }
        }
    }

    /// One of the silenced call's options was chosen.
    function instead(what) {
        if (what === "message") {
            page.center.messageInstead()
        } else if (what === "remind") {
            page.center.remindLater()
        } else {
            page.call.hang_up()
        }
    }

    // Hanging up, at the foot of the screen, where the phone's own is.
    Button {
        objectName: "hangUpButton"
        visible: page.inCall
        anchors {
            bottom: parent.bottom
            bottomMargin: Theme.itemSizeLarge
            horizontalCenter: parent.horizontalCenter
        }
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
