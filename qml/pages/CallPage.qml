import QtQuick 2.0
import Sailfish.Silica 1.0
import QtSensors 5.0
import "../components"

/*
 * Call app is in: ringing, setting up, in progress, over. Call is window's (`CallCenter.qml`).
 * Phone call screen layout: who top left, time right, duration large, switches, red button at
 * foot; their picture blurred behind.
 *
 * Media runs in upstream's `CallView.qml`, unseen; controls drawn here (mic via call, which
 * presses page's switch; hang up via call).
 *
 * Pictures on: pictures only (call_video.js), lines/switches over, tap toggles. Duration
 * replaces time; large clock goes. Camera switch in every call (voice can become video) and
 * ringing video call (answer camera-off). Own picture top right once theirs drawn, with turn
 * button; spinner over black picture until shown.
 *
 * Loaded, not declared: no browser engine = call lost, page kept (ringing still declinable).
 * Not swipeable while call up (drops media); page goes itself once over.
 */
Page {
    id: page

    /// Window's call.
    property QtObject call: null

    /// Call sound output (CallAudio), handed over when ready.
    property QtObject audio: null

    /// Anything short of over.
    readonly property bool busy: page.call !== null && page.call.state !== ""
                                 && page.call.state !== "ended"
    /// Both agreed to talk: call page takes over.
    readonly property bool inCall: page.busy && page.call.state !== "ringing"

    /// Ringing video call: camera off before answer possible.
    readonly property bool ringingVideo: page.call !== null && page.call.state === "ringing"
                                         && page.call.has_video
    /// Pictures on screen: either end's camera.
    readonly property bool video: page.inCall
                                  && (page.call.local_video || page.call.remote_video)
    /// Lines/switches over pictures: tap toggles; auto-hide when both seen.
    property bool controlsShown: true
    /// Drawn over call now.
    readonly property bool overlay: !page.video || page.controlsShown
    /// Camera turning until other camera's first frame (1-2 s). Button dims in place (nothing
    /// moves under finger); spinner from press.
    property bool turning: false

    /// App in front. Writable: tests (no app in front) set it.
    property bool appActive: Qt.application.state === Qt.ApplicationActive

    /// Now, ticked each second while clock shown.
    property real now: Date.now()

    /// Cover quick actions must not jump away from this.
    readonly property bool pausesQuickActions: page.busy

    backNavigation: !page.busy

    /// Connected; clock running.
    readonly property bool connected: page.call !== null && page.call.state === "connected"

    /// Connected duration: h, m, s, two digits.
    function elapsed() {
        if (!page.call || page.call.connected_at <= 0) {
            return ""
        }
        var seconds = Math.max(0, Math.floor(page.now / 1000 - page.call.connected_at))
        var parts = [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60]
        return parts.map(function (part) { return (part < 10 ? "0" : "") + part }).join(":")
    }

    /// As phone app: zero groups quieter.
    function elapsedMarkup() {
        var text = page.elapsed()
        var lead = /^(00:){0,2}/.exec(text)[0]
        if (lead.length === 0) {
            return text
        }
        return "<font color=\"" + Theme.secondaryHighlightColor + "\">" + lead + "</font>"
                + text.substring(lead.length)
    }

    /// Call state in Delta Chat's strings. Not `status` (page's own).
    function describe() {
        if (!page.call) {
            return ""
        }
        var state = page.call.state
        if (state === "ringing") {
            return page.call.has_video ? qsTr("Incoming video call") : qsTr("Incoming call")
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

    // Lines/switches hide when both pictures up, reader idle; back on tap or when pictures go.
    Timer {
        id: tuck
        objectName: "tuck"
        interval: 5000
        running: page.video && page.controlsShown && page.call.remote_video
                 && page.connected
        onTriggered: page.controlsShown = false
    }

    onVideoChanged: {
        if (!page.video) {
            page.controlsShown = true
        }
    }

    /// Touch: lines stay longer.
    function touched() {
        page.controlsShown = true
        tuck.restart()
    }

    /// Turn camera. `turning` set after `flip_camera`: call reports facing at once, so later
    /// updates are the turn.
    function turn() {
        page.touched()
        page.call.flip_camera()
        page.turning = true
        turnLimit.restart()
    }

    /// Turn over: other camera shows, or none to wait for.
    function lookAtTurn() {
        if (page.call === null || page.call.local_painted || !page.call.camera) {
            page.turning = false
        }
    }

    // Cap: button not left dimmed if no frame comes.
    Timer {
        id: turnLimit
        objectName: "turnLimit"
        interval: 10000
        onTriggered: page.turning = false
    }

    /// Tap on pictures: lines/switches away or back.
    function toggleControls() {
        if (!page.video) {
            return
        }
        if (page.controlsShown) {
            page.controlsShown = false
        } else {
            page.touched()
        }
    }

    // Clock only while app in front: background timer wakes phone each second. Restart reads
    // time at once (`triggeredOnStart`).
    Timer {
        objectName: "clock"
        interval: 1000
        repeat: true
        running: page.call !== null && page.call.state !== "" && page.appActive
        triggeredOnStart: true
        onTriggered: page.now = Date.now()
    }

    // Over: said briefly, page goes. Only when on screen (permission prompt goes first), so pop
    // takes this page.
    property bool closeWhenActive: false

    /// Leaving: popped, or swiped once over. New call gets own page after (CallCenter.qml).
    property bool leaving: false

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
        page.leaving = true
        if (page.call && page.call.state === "ended") {
            page.call.reset()
        }
        pageStack.pop()
    }

    onStatusChanged: {
        if (page.status === PageStatus.Active) {
            page.leaving = false
            if (page.closeWhenActive) {
                page.close()
            }
        } else if (page.status === PageStatus.Deactivating && !page.busy) {
            // Only ended call swipeable.
            page.leaving = true
        }
    }

    // Call arriving while last shown as over keeps page, unless page leaving (would take it).
    onBusyChanged: {
        if (page.busy && !page.leaving) {
            leave.stop()
            page.closeWhenActive = false
        }
    }

    Connections {
        target: page.call
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onEnded: leave.restart()
        onVideo_changed: page.lookAtTurn()
    }

    // Over when pushed (failed, or ended while stack busy): said, gone.
    Component.onCompleted: {
        if (page.call && page.call.state === "ended") {
            leave.restart()
        }
    }

    // Leaving drops media: hang up is honest end. Only with media here, page not leaving.
    Component.onDestruction: {
        if (page.inCall && !page.leaving) {
            page.call.hang_up()
        }
    }

    // Their picture: full screen, blurred, dimmed; none if none.
    Item {
        id: backdrop
        objectName: "callBackdrop"
        anchors.fill: parent
        visible: picture.status === Image.Ready && !page.video

        // Blurred small off main thread, drawn large (src/pictures.rs). Not FastBlur: shader made
        // after window hidden (background ring) can draw with another shader's program on phone's
        // Qt. See Avatar.qml.
        Image {
            id: picture
            anchors.fill: parent
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            smooth: true
            source: page.call && page.call.peer_avatar.length > 0
                    ? "image://piirit/blur?file="
                      + encodeURIComponent(page.call.peer_avatar)
                    : ""
        }

        Rectangle {
            anchors.fill: parent
            color: Theme.rgba(Theme.highlightDimmerColor, 0.6)
        }
    }

    // Media, only its pictures shown, controls here. Unseen, not hidden (hidden pauses engine).
    Loader {
        id: view
        objectName: "callViewLoader"
        anchors.fill: parent
        opacity: page.video ? 1 : 0
        active: page.call !== null && page.call.url.length > 0
        source: Qt.resolvedUrl("../components/CallView.qml")
        onLoaded: {
            // Before `url`: page loads only once call sound has output.
            view.item.allowed = Qt.binding(function () {
                return page.audio === null || page.audio.ready
            })
            view.item.holding = Qt.binding(function () { return page.busy })
            // What WebView.qml would bind `active` to.
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

    // Unseen page takes taps on undrawn buttons: shield catches. Over pictures, tap toggles.
    MouseArea {
        objectName: "callViewShield"
        anchors.fill: parent
        onClicked: page.toggleControls()
    }

    // Black picture before first frame: spinner. Their end fills screen; ours until theirs.
    BusyIndicator {
        objectName: "theirSpinner"
        anchors.centerIn: parent
        size: BusyIndicatorSize.Large
        running: page.video && page.call.remote_video && !page.call.remote_painted
    }

    BusyIndicator {
        objectName: "ourSpinner"
        anchors.centerIn: ownPicture.cornered ? ownPicture : parent
        size: ownPicture.cornered ? BusyIndicatorSize.Medium : BusyIndicatorSize.Large
        running: page.video && page.call.local_video
                 && (!page.call.local_painted || page.turning)
    }

    // Own picture top right, size from call_video.js. Taps fall to shield.
    Item {
        id: ownPicture
        objectName: "ownPicture"

        /// In corner, not full screen.
        readonly property bool cornered: page.video && page.call.local_video
                                         && page.call.remote_video

        visible: ownPicture.cornered
        // Corner (call_video.js): 27% wide, 36% tall, 4% right, 10% down.
        width: page.width * 0.27
        height: page.width * 0.36
        x: page.width * 0.69
        y: page.height * 0.1

        // Turn button, lower-left of picture: only while cornered and shown. Hit area > disc.
        Rectangle {
            objectName: "pictureFlip"
            visible: ownPicture.cornered && page.overlay && page.call.camera
                     && (page.call.local_painted || page.turning)
            opacity: page.turning ? Theme.opacityLow : 1.0
            width: Theme.iconSizeSmallPlus
            height: Theme.iconSizeSmallPlus
            radius: width / 2
            color: Theme.rgba("black", 0.45)
            anchors {
                left: parent.left
                leftMargin: Theme.paddingMedium
                bottom: parent.bottom
                bottomMargin: Theme.paddingMedium
            }

            IconButton {
                id: pictureFlipButton
                objectName: "pictureFlipButton"
                anchors.centerIn: parent
                width: Theme.iconSizeMedium
                height: Theme.iconSizeMedium
                enabled: !page.turning
                icon.source: "image://theme/icon-camera-switch"
                icon.width: Theme.iconSizeSmall
                icon.height: Theme.iconSizeSmall
                onClicked: page.turn()
            }
        }
    }

    // Top/foot shade for legibility. Gradient, not blur: see backdrop.
    Rectangle {
        objectName: "topShade"
        anchors {
            top: parent.top
            left: parent.left
            right: parent.right
        }
        height: who.y + who.height + statusLine.height + 2 * Theme.paddingLarge
        visible: page.video && page.overlay
        gradient: Gradient {
            GradientStop { position: 0.0; color: Theme.rgba("black", 0.6) }
            GradientStop { position: 1.0; color: "transparent" }
        }
    }

    Rectangle {
        objectName: "footShade"
        anchors {
            bottom: parent.bottom
            left: parent.left
            right: parent.right
        }
        height: parent.height - switches.y + Theme.paddingLarge
        visible: page.video && page.overlay
        gradient: Gradient {
            GradientStop { position: 0.0; color: "transparent" }
            GradientStop { position: 1.0; color: Theme.rgba("black", 0.6) }
        }
    }

    // Who (left), time (right): as phone call screen.
    Item {
        id: who
        objectName: "callWho"
        anchors {
            top: parent.top
            topMargin: 2 * Theme.paddingLarge
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        height: name.height
        visible: page.overlay

        Label {
            id: name
            objectName: "callName"
            width: parent.width - clock.width - Theme.paddingLarge
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.primaryColor
            // Their own name for themselves.
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
            // Message-time form; over pictures, duration.
            text: page.video && page.connected ? page.elapsed()
                                               : Qt.formatTime(new Date(page.now), "hh:mm")
        }
    }

    // Duration once both hear; before: state; after: over. Clock-tall line: switches stay put.
    Item {
        id: statusLine
        anchors {
            top: who.bottom
            topMargin: Theme.itemSizeLarge
        }
        width: parent.width
        height: clockMetric.height
        visible: !page.video || (page.controlsShown && !page.connected)

        // One line of clock font, measured.
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
            // Markup only for own clock digits.
            textFormat: page.connected ? Text.StyledText : Text.PlainText
            text: page.connected ? page.elapsedMarkup() : page.describe()
        }
    }

    // Switches under clock; over pictures, above red button. Mic, speaker, camera (no keypad or
    // recording: phone calls only). Ringing video: camera alone.
    Row {
        id: switches
        objectName: "callSwitches"
        visible: (page.inCall || page.ringingVideo) && page.overlay
        anchors.horizontalCenter: parent.horizontalCenter
        y: page.video ? hangUp.y - height - 2 * Theme.paddingLarge
                      : statusLine.y + statusLine.height + 2 * Theme.paddingLarge
        spacing: Theme.paddingLarge

        Switch {
            id: mute
            objectName: "muteSwitch"
            visible: page.inCall
            icon.source: "image://theme/icon-m-mic-mute"
            automaticCheck: false
            checked: page.call !== null && page.call.muted
            onClicked: {
                page.touched()
                page.call.mute(!page.call.muted)
            }
        }

        // Speaker/earpiece. Busy until moved (CallAudio.qml). Dimmed if plugged/paired; for good
        // if audio policy refused.
        Switch {
            objectName: "speakerSwitch"
            visible: page.inCall && page.audio !== null
            icon.source: "image://theme/icon-m-speaker"
            automaticCheck: false
            busy: page.audio !== null && page.audio.switching
            enabled: page.audio !== null && (page.audio.switchable || page.audio.switching)
            checked: page.audio !== null && page.audio.speaker
            onClicked: {
                page.touched()
                page.audio.choose(!page.audio.speaker)
            }
        }

        Switch {
            objectName: "cameraSwitch"
            icon.source: "image://theme/icon-m-video"
            automaticCheck: false
            checked: page.call !== null && page.call.camera
            onClicked: {
                page.touched()
                page.call.set_camera(!page.call.camera)
            }
        }
    }

    // Decline left, answer right while ringing, as phone.
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

    // Hang up at screen foot, as phone.
    Button {
        id: hangUp
        objectName: "hangUpButton"
        visible: page.inCall && page.overlay
        anchors {
            bottom: parent.bottom
            bottomMargin: Theme.itemSizeLarge
            horizontalCenter: parent.horizontalCenter
        }
        //: Ends a call in progress.
        text: qsTr("End call")
        onClicked: page.call.hang_up()
    }

    // Held to ear: cheek mustn't touch. App can't blank display: black shield eats touch. Not
    // while ringing or video (held in front).
    ProximitySensor {
        id: proximity
        objectName: "proximity"
        active: page.inCall && !page.video && !page.call.camera && page.appActive
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
