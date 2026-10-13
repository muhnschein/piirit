import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../components"
import "../js/QuickActions.js" as QuickActions
import "../js/Media.js" as Media

/*
 * What the cover has to say while the app is minimised: who is there,
 * and whether any of them has said something new.
 *
 * The whole cover is a staggered grid of circles, everyone's avatar in
 * grey, once each, and an empty circle wherever there are more cells
 * than people -- so a fresh profile with nobody in it is a field of
 * empty circles rather than a line of text. The grid starts above the
 * cover's top edge, so the first row shows only its lower part and the
 * eye lands on the rows below it. Whoever has written is drawn in the
 * ambience's own highlight colour, in the cells that are seen whole and
 * nearest the top; and only then is the count drawn, as a pill the
 * width of two cells in the middle of the second row, saying how many
 * messages are new, with the lit faces under it. Nothing new, and the
 * cover is the grid alone. With more people than cells, whoever has a
 * picture is drawn before whoever has only an initial: a cover of
 * faces says more than a cover of letters.
 *
 * Every profile counts: one ChatList per configured profile, so the
 * number is every unread message on the phone and the grid is everyone
 * the reader talks to, whichever profile they are under. The lists are
 * the cover's own rather than the chat list page's, because a cover
 * outlives any page -- the app can be minimised from anywhere, including
 * onboarding. The people come out of each list as one JSON list
 * (`cover_people`): the grid is laid out in a pass over them, which a
 * view over the rows could not do.
 *
 * At its foot, the one or two quick actions the reader chose in the
 * settings -- with one chosen, the left one alone: a chat of any
 * profile, the search, this profile's QR code, the scanner, or the list
 * of profiles. The home screen draws them, in the strip along the bottom
 * edge, and while they are there the grid sinks away into that strip
 * rather than running under the icons -- the faces fade out towards the
 * bottom, nothing is laid over them. What a tap does is the window's (piirit.qml); the cover
 * only says which one it was.
 *
 * While a call is up -- ringing here, being placed, or in progress --
 * the cover is the call's, and the grid, its count and those actions
 * give way to it. The other end's picture fills it, larger than the
 * cover and faint, under a shade that is darkest at the top, where
 * their name is, and at the foot, where the home screen draws the
 * action, and lightest over their face. Without a picture it is the
 * plain cover with their avatar, the circle of their colour and
 * initial, over the name. Under the name, how long the call has gone
 * on, or how it stands until it is connected, in the call page's own
 * words; in the middle, what the reader may want to know at a glance,
 * each on a pill of its own: that the microphone is off, and that the
 * camera, which is let go while the app is in the background, is
 * paused. And one action, the microphone: off, or on again. Not
 * hanging up, which a tap meant for something else would do by
 * mistake, and not the camera, which stays let go while the cover is
 * what is on screen. A call ringing unanswered has no action and no pills:
 * there is nothing yet to switch, and it is answered on its page.
 *
 * The picture is the plain file, the face as it is, rather than the
 * blurred copy the call page is drawn over: on a cover a face out of
 * focus is hardly one, and who is on the call is what it is there to
 * show. Faded and shaded, not blurred by a shader either: nothing on
 * the cover is drawn by one (Avatar.qml says why).
 */
CoverBackground {
    id: cover

    // One list per profile. A Repeater's delegates have to be Items,
    // so each list sits in an empty one.
    Repeater {
        id: lists
        model: core.account_list

        delegate: Item {
            visible: false
            width: 0
            height: 0
            property alias chats: chats

            ChatList {
                id: chats
                objectName: "coverChats"
                account_id: model.is_configured ? model.account_id : 0
                onRows_changed: cover.gather()
            }
        }
    }

    Connections {
        target: core
        onAccounts_refreshed: cover.gather()
        onCore_event: {
            for (var i = 0; i < lists.count; i++) {
                lists.itemAt(i).chats.handle_event(context_id, kind, payload_json)
            }
        }
    }

    Component.onCompleted: {
        cover.ready = true
        core.refresh_accounts()
        cover.gather()
    }

    /// Everyone, across every profile, in each list's order.
    property var people: []
    /// Unread messages across every profile.
    property int unreadTotal: 0
    /// Whether the app can take a quick action now. The window says: not
    /// before there is a chat list to land on, and not while a page is up
    /// that must not be jumped away from.
    property bool quickActionsAllowed: true

    /// A quick action was tapped: `side` is "left" or "right", as Settings
    /// keeps them.
    signal quickAction(string side)

    readonly property var leftAction: QuickActions.shown(Settings, "left")
    readonly property var rightAction: QuickActions.shown(Settings, "right")
    /// Whether the home screen is drawing any of the reader's actions: one
    /// set, the app able to take it, and no call on the cover, which has
    /// its own.
    readonly property bool actionsShown: cover.quickActionsAllowed && !cover.callUp
        && (cover.leftAction.kind !== "" || cover.rightAction.kind !== "")

    /// An icon's picture, as a whole URL: the home screen reads the file
    /// itself, from outside the app.
    function iconUrl(name) {
        return Qt.resolvedUrl("../" + QuickActions.iconFile(
            name, Theme.iconSizeSmall, QuickActions.isLight(Theme.primaryColor)))
    }

    /// The picture for an action. None for no action.
    function actionIcon(action) {
        if (action.kind === "") {
            return ""
        }
        return cover.iconUrl(QuickActions.iconName(action))
    }

    /// The window's call (CallCenter.qml), handed over so the cover can
    /// show it; null for none.
    property QtObject call: null

    /// A call is up: ringing here, being placed, or in progress -- as long
    /// as the call page keeps itself on the stack. The cover is the
    /// call's for that time.
    readonly property bool callUp: cover.call !== null && cover.call.state !== ""
                                   && cover.call.state !== "ended"
    /// The call is past ringing here: there is a microphone to switch, and
    /// a camera that may have been let go. Placed calls are from the
    /// start.
    readonly property bool callUnderWay: cover.callUp && cover.call.state !== "ringing"

    /// The other end's picture, a file; empty for none.
    readonly property string callPicturePath: cover.callUp ? "" + cover.call.peer_avatar : ""
    /// Their picture is what the call is drawn over: there is one, and it
    /// has loaded. Until then, and for a file that never loads, the plain
    /// cover -- as an avatar is its colour and initial until its picture
    /// is ready.
    readonly property bool callPictured: cover.callUp && callPicture.status === Image.Ready

    /// What the call is drawn over, under the shade and behind the pills:
    /// black in a dark ambience, white in a light one, so the ambience's
    /// own text reads on it either way.
    readonly property color callGround: QuickActions.isLight(Theme.primaryColor)
                                        ? Qt.rgba(0, 0, 0, 1) : Qt.rgba(1, 1, 1, 1)

    /// Now, moved on once a second while the call's clock is seen.
    property real now: Date.now()

    /// How long the call has been connected, as the call page counts it
    /// (CallPage.qml): hours, minutes and seconds, two digits each.
    function callElapsed() {
        if (!cover.callUp || cover.call.connected_at <= 0) {
            return ""
        }
        var seconds = Math.max(0, Math.floor(cover.now / 1000 - cover.call.connected_at))
        var parts = [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60]
        return parts.map(function (part) { return (part < 10 ? "0" : "") + part }).join(":")
    }

    /// What is said under the name: how long, once the two ends hear each
    /// other, and how the call stands until then. The call page's own
    /// words, asked for in its context, so they are translated wherever
    /// the call page is.
    function callStatus() {
        if (!cover.callUp) {
            return ""
        }
        var state = cover.call.state
        if (state === "ringing") {
            return cover.call.has_video ? qsTranslate("CallPage", "Incoming video call")
                                        : qsTranslate("CallPage", "Incoming call")
        }
        if (state === "calling") {
            return qsTranslate("CallPage", "Ringing…")
        }
        if (state === "starting" || state === "connecting") {
            return qsTranslate("CallPage", "Connecting…")
        }
        if (state === "reconnecting") {
            return qsTranslate("CallPage", "Reconnecting…")
        }
        return cover.callElapsed()
    }

    // The call's clock, only while it is seen. Most of a call nobody is
    // looking at the cover -- the phone is at an ear, or its screen is off
    // -- and a timer that ran through all of it would wake the phone once
    // a second for nothing.
    Timer {
        id: callClock
        objectName: "callClock"
        interval: 1000
        repeat: true
        running: cover.looking && cover.callUp && cover.call.state === "connected"
        onTriggered: cover.now = Date.now()
        // Then and there rather than on the first tick, which comes later:
        // a cover looked at again does not say the time it went away.
        onRunningChanged: {
            if (callClock.running) {
                cover.now = Date.now()
            }
        }
    }

    /// How tall the strip is that the home screen draws the actions in. A
    /// cover cannot ask, so this is measured rather than derived, the way
    /// vuo's cover measured it: the icons' tops sit about 18% of the
    /// cover's height up, which a small item's height clears.
    readonly property real actionStrip: Theme.itemSizeSmall
    /// Where the faces stop being seen whole: above the strip while there
    /// are actions in it, and the bottom edge otherwise.
    readonly property real floor: cover.actionsShown
                                  ? cover.height - cover.actionStrip
                                  : cover.height
    /// The band the faces fade out over while there are actions: from
    /// twice the strip's height above the bottom edge, to the edge.
    readonly property real fadeFrom: cover.height - 2 * cover.actionStrip
    readonly property real fadeTo: cover.height
    /// What the grid draws: `{person, row, col, loud}` per cell, `person`
    /// null for an empty circle and `loud` on the cell of each person
    /// with something new.
    property var cells: []

    /// The grid's shape: three across, with every other row shifted half
    /// a cell and holding one more, cut off at both edges -- so the rows
    /// nest, and the grid reads as a field of faces rather than a table.
    /// The first row is a whole one, so the shifted row under it is where
    /// the pill goes: its two middle cells are whole and centred.
    readonly property int columns: 3
    readonly property int cellSize: Math.floor(cover.width / cover.columns)
    /// The room between two circles, and between a circle and the pill.
    readonly property int gap: Theme.paddingMedium
    readonly property int rowStep: Math.max(1, Math.round(cover.cellSize * 0.95))
    /// How far above the top edge the grid starts, so the first row is
    /// seen only from the waist down.
    readonly property int topCut: Math.round(cover.cellSize * 0.6)
    readonly property int rows: cover.cellSize > 0
                                ? Math.ceil((cover.height + cover.topCut) / cover.rowStep)
                                : 0

    /// Whether a row is the shifted kind: one more cell, half a cell to
    /// the left.
    function shifted(row) {
        return row % 2 === 1
    }

    /// How many cells a row has.
    function across(row) {
        return cover.shifted(row) ? cover.columns + 1 : cover.columns
    }

    /// Where a cell is, across the grid.
    function cellX(row, col) {
        return col * cover.cellSize
               - (cover.shifted(row) ? cover.cellSize / 2 : 0)
    }

    /// Where a row is, down the grid.
    function cellY(row) {
        return row * cover.rowStep - cover.topCut
    }

    /// The row the pill is in, and the two cells it takes: the middle
    /// pair of the first shifted row, which the shift puts on either
    /// side of the centre line.
    readonly property int pillRow: 1
    readonly property int pillFirstCol: cover.across(cover.pillRow) / 2 - 1
    /// The pill is there only when it has something to say.
    readonly property bool showPill: cover.unreadTotal > 0

    /// Whether a cell is one of the two the pill is drawn over.
    function underPill(row, col) {
        return row === cover.pillRow
               && (col === cover.pillFirstCol || col === cover.pillFirstCol + 1)
    }

    /// How good a cell is to be seen in, smaller being better.
    ///
    /// A cover is glanced at, so a face that matters cannot be one of
    /// the halves the shifted rows leave hanging off an edge, the row the
    /// top cuts through, or the part-row the bottom cuts through -- or,
    /// with quick actions, the one fading into their strip. What is
    /// left is ranked by how far down the cover it is and then by how far
    /// out from the middle, which is the order an eye takes them in.
    function prominence(row, col) {
        var size = cover.cellSize
        var x = cover.cellX(row, col)
        var y = cover.cellY(row)
        var whole = x >= 0 && x + size <= cover.width
                    && y >= 0 && y + size <= cover.floor
        var fromMiddle = Math.abs((x + size / 2) - cover.width / 2) / size
        return (whole ? 0 : 100) + row * 2 + fromMiddle
    }

    /// Whether the cover is the thing being looked at.
    ///
    /// A cover is drawn only when the app is minimised, the home screen is
    /// showing and the display is on. The rest of the time -- which is most
    /// of it, and all of the night -- the app is still receiving and the
    /// lists below are still following every arrival, but nothing is on
    /// screen to redraw.
    readonly property bool looking: cover.status === Cover.Active

    /// Whether the grid is what is being looked at: the cover is, and no
    /// call has it. The lists follow every arrival through a call as well;
    /// the grid is laid out again once it is seen again.
    readonly property bool gridSeen: cover.looking && !cover.callUp

    /// Something changed while nothing was looking, so the cells are not
    /// the current answer. True to start with: nothing has been read yet.
    property bool stale: true

    /// Whether the cover is built. Its size and status arrive one
    /// property at a time while it is being made, and each arrival
    /// would lay the grid out again -- once with whatever status the
    /// cover had before it was told its own.
    property bool ready: false

    /// Read the lists again if there is anyone to read them for, and
    /// otherwise remember that they want reading.
    ///
    /// Called on every change to any list and whenever the shape changes,
    /// so the cells are the right number by the time they are drawn.
    function gather() {
        if (!cover.ready || !cover.gridSeen) {
            cover.stale = true
            return
        }
        cover.stale = false
        cover.rebuild()
    }

    // Whatever was missed while the cover was away, or a call had it,
    // done once on the way back rather than once per arrival while it was
    // gone.
    onGridSeenChanged: {
        if (cover.gridSeen && cover.stale) {
            cover.gather()
        }
    }

    /// The order people take cells in: whoever has something new, then
    /// whoever has a picture, then everyone else -- each group in the
    /// lists' order.
    function rank(person) {
        if (person.unread_count > 0) {
            return 0
        }
        return ("" + person.avatar_path).length > 0 ? 1 : 2
    }

    /// Who is there, how much is unread, and what fills the grid.
    function rebuild() {
        var everyone = []
        var total = 0
        for (var i = 0; i < lists.count; i++) {
            var chats = lists.itemAt(i).chats
            total += chats.unread_total
            var some = []
            try {
                some = JSON.parse(chats.cover_people)
            } catch (err) {
                some = []
            }
            for (var j = 0; j < some.length; j++) {
                // Keyed by profile too: two profiles can hold the same
                // chat id.
                some[j].key = i + ":" + some[j].chat_id
                everyone.push(some[j])
            }
        }

        // The cells there are, in the order they are drawn -- less the
        // two the pill sits on, when it is there.
        var slots = []
        for (var row = 0; row < cover.rows; row++) {
            for (var col = 0; col < cover.across(row); col++) {
                if (total > 0 && cover.underPill(row, col)) {
                    continue
                }
                slots.push({ row: row, col: col, person: null, loud: false })
            }
        }

        // Everyone once, in rank order. A chat id met twice under one
        // profile is one person.
        var placed = []
        var seen = {}
        for (var wanted = 0; wanted <= 2; wanted++) {
            for (var p = 0; p < everyone.length; p++) {
                var person = everyone[p]
                if (seen[person.key] || cover.rank(person) !== wanted) {
                    continue
                }
                seen[person.key] = true
                placed.push(person)
            }
        }

        // They get the cells worth having, best first. Sorted apart from
        // `slots` so the grid is still drawn in its own order; whatever
        // is left over stays an empty circle.
        var best = slots.slice()
        best.sort(function (one, other) {
            return cover.prominence(one.row, one.col)
                   - cover.prominence(other.row, other.col)
        })
        for (var b = 0; b < placed.length && b < best.length; b++) {
            best[b].person = placed[b]
            best[b].loud = placed[b].unread_count > 0
        }

        var made = []
        for (var s = 0; s < slots.length; s++) {
            var slot = slots[s]
            made.push({ person: slot.person, row: slot.row, col: slot.col,
                        loud: slot.loud })
        }

        cover.people = everyone
        cover.unreadTotal = total
        cover.cells = made
    }
    onRowsChanged: cover.gather()
    onWidthChanged: cover.gather()
    // The cells worth having move with the strip.
    onFloorChanged: cover.gather()

    // The grid, filling the cover and running past every edge: half a
    // cell at the sides on the shifted rows, most of a cell at the top,
    // and whatever the bottom cuts through.
    Item {
        id: grid
        objectName: "avatarGrid"
        anchors.fill: parent
        clip: true
        visible: !cover.callUp

        Repeater {
            model: cover.cells

            Avatar {
                objectName: "gridCell"
                x: cover.cellX(modelData.row, modelData.col) + cover.gap / 2
                y: cover.cellY(modelData.row) + cover.gap / 2
                width: cover.cellSize - cover.gap
                /// Whether anyone is drawn here, or only the circle.
                readonly property bool filled: modelData.person !== null
                initial: filled ? modelData.person.name : ""
                ownColor: filled ? modelData.person.color : ""
                picturePath: filled ? modelData.person.avatar_path : ""
                // Nobody is drawn in their own colours here. A cover is
                // the phone's, not the app's: whoever has written is the
                // ambience's highlight, the same colour the unread badge
                // in the chat list wears, everyone else is grey, and an
                // empty circle is fainter still.
                monochrome: !modelData.loud
                highlight: modelData.loud
                opacity: modelData.loud ? 1.0 : filled ? 0.6 : 0.35
                z: modelData.loud ? 1 : 0
                // The room for the actions, made by the grid rather than
                // over it: towards the bottom edge the faces run out, over
                // a band twice the strip's height, so the icons sit on the
                // cover's own ground with the faces nearly gone around
                // them (QuickActions.fade says how). Each face is told
                // where the band crosses it, as fractions of its own
                // height. Off while there are no actions, so the faces run
                // whole to the edge.
                //
                // Each face fades itself rather than the grid being drawn
                // through one fade: that was a layer and a ShaderEffect,
                // and on the phone's Qt the faces made after the cover had
                // once been hidden were drawn with the fade's program
                // instead of their own -- white squares. See Avatar.qml.
                fadeFrom: cover.actionsShown ? (cover.fadeFrom - y) / Math.max(1, height) : 0
                fadeTo: cover.actionsShown ? (cover.fadeTo - y) / Math.max(1, height) : 0
            }
        }

        // The count, in a cell of its own two cells wide: a circle
        // stretched sideways, so it sits among the faces as one of them
        // rather than over them. There only when something is new, and
        // then in the highlight, the way a face with something new is
        // -- as an outline with the number in the same colour, rather
        // than a filled disc: the number is what the pill is there to
        // say, and in the ambience's own text colour on a disc of its
        // highlight it was the harder of the two to read.
        Rectangle {
            id: pill
            objectName: "unreadPill"
            visible: cover.showPill
            x: cover.cellX(cover.pillRow, cover.pillFirstCol) + cover.gap / 2
            y: cover.cellY(cover.pillRow) + cover.gap / 2
            width: 2 * cover.cellSize - cover.gap
            height: cover.cellSize - cover.gap
            radius: height / 2
            color: "transparent"
            border.width: Math.max(2, Math.round(Theme.paddingSmall / 2))
            border.color: Theme.highlightColor
            z: 2

            Label {
                objectName: "unreadTotal"
                anchors {
                    fill: parent
                    leftMargin: Theme.paddingMedium
                    rightMargin: Theme.paddingMedium
                }
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeLarge
                // Shrunk to fit rather than cut: "99+ new" in a language
                // with a long word for new still has to be read whole.
                fontSizeMode: Text.HorizontalFit
                minimumPixelSize: Theme.fontSizeTiny
                //: On the cover, in a pill among the avatars: how many
                //: messages are unread. %1 is the number, or "99+".
                text: qsTr("%1 new").arg(cover.unreadTotal > 99 ? "99+"
                                                                  : cover.unreadTotal)
            }
        }
    }

    // The call, in the grid's place while one is up.
    Item {
        id: callView
        objectName: "callView"
        anchors.fill: parent
        clip: true
        visible: cover.callUp

        // Their face, larger than the cover and a little above its
        // middle, where the shade over it is lightest. Faint: it says who
        // is on the call rather than being looked at, and the words are
        // read over it. Read off the main thread, at the size it is drawn.
        Image {
            id: callPicture
            objectName: "callPicture"
            width: Math.round(parent.width * 1.2)
            height: Math.round(parent.height * 1.2)
            x: Math.round((parent.width - width) / 2)
            y: Math.round((parent.height - height) / 2 - parent.height * 0.08)
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            smooth: true
            sourceSize.height: height
            opacity: 0.4
            visible: cover.callPictured
            source: cover.callPicturePath.length > 0 ? Media.fileUrl(cover.callPicturePath) : ""
        }

        // The shade, so the name and the action read over any face: dark
        // at the top and darker at the foot, nearly clear between. A
        // gradient, not a blur; see the picture above.
        Rectangle {
            objectName: "callShade"
            anchors.fill: parent
            visible: cover.callPictured
            gradient: Gradient {
                GradientStop { position: 0.0; color: cover.shade(0.6) }
                GradientStop { position: 0.45; color: cover.shade(0.1) }
                GradientStop { position: 0.6; color: cover.shade(0.15) }
                GradientStop { position: 1.0; color: cover.shade(0.85) }
            }
        }

        // Who, near the top, large: it is what the cover is for while the
        // call is up. Over the name, without a picture, their avatar.
        Column {
            id: callHead
            objectName: "callHead"
            anchors {
                top: parent.top
                topMargin: cover.callPictured ? 2 * Theme.paddingLarge : Theme.paddingLarge
                left: parent.left
                leftMargin: Theme.paddingMedium
                right: parent.right
                rightMargin: Theme.paddingMedium
            }
            spacing: Theme.paddingSmall

            Avatar {
                objectName: "callAvatar"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !cover.callPictured
                width: Math.round(cover.width * 0.4)
                initial: cover.callUp ? cover.call.peer_name : ""
                ownColor: cover.callUp ? cover.call.peer_color : ""
            }

            // A little more room between the avatar and the name than
            // between the name and the time, which belong together.
            Item {
                visible: !cover.callPictured
                width: 1
                height: Theme.paddingSmall
            }

            Label {
                objectName: "callName"
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                maximumLineCount: 2
                truncationMode: TruncationMode.Elide
                font.pixelSize: Theme.fontSizeLarge
                color: Theme.primaryColor
                // The other end's own name for themselves.
                textFormat: Text.PlainText
                text: cover.callUp ? cover.call.peer_name : ""
            }

            Label {
                objectName: "callTime"
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeMedium
                color: Theme.highlightColor
                textFormat: Text.PlainText
                text: cover.callStatus()
            }
        }

        // What is so of the call, a pill each, in the middle of the room
        // between the name and the action.
        Column {
            id: callNotes
            objectName: "callNotes"
            readonly property real roomTop: callHead.y + callHead.height + Theme.paddingLarge
            readonly property real roomBottom: cover.height
                                               - (cover.callUnderWay ? cover.actionStrip : 0)
            anchors.horizontalCenter: parent.horizontalCenter
            y: Math.round(callNotes.roomTop
                          + Math.max(0, (callNotes.roomBottom - callNotes.roomTop - height) / 2))
            spacing: Theme.paddingMedium

            // Something the reader can undo, from right here: loud.
            CallNote {
                objectName: "micNote"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: cover.callUnderWay && cover.call.muted
                loud: true
                ground: cover.callGround
                maximumWidth: cover.width - 2 * Theme.paddingMedium
                iconName: "icon-m-mic-mute"
                //: On the cover during a call: the reader's microphone is
                //: switched off, so the other end hears nothing.
                text: qsTr("Microphone off")
            }

            // Wanted, and let go while the app is in the background --
            // which, the cover being seen, it is. Back by itself once the
            // app is in front, so only said.
            CallNote {
                objectName: "cameraNote"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: cover.callUnderWay && cover.call.camera
                ground: cover.callGround
                maximumWidth: cover.width - 2 * Theme.paddingMedium
                iconName: "icon-m-video"
                //: On the cover during a video call: the camera sends
                //: nothing while the app is in the background, and comes
                //: back on when the app is opened.
                text: qsTr("Camera paused")
            }
        }
    }

    /// The call's shade at `strength`, in its ground's colour.
    function shade(strength) {
        var ground = cover.callGround
        return Qt.rgba(ground.r, ground.g, ground.b, strength)
    }

    // The call's one action, the microphone: off, or on again. Its icon
    // is what a tap does.
    CoverActionList {
        objectName: "callActions"
        enabled: cover.callUnderWay

        CoverAction {
            objectName: "micAction"
            iconSource: cover.callUp ? cover.iconUrl(cover.call.muted ? "mic" : "mic-off") : ""
            onTriggered: {
                if (cover.callUnderWay) {
                    cover.call.mute(!cover.call.muted)
                }
            }
        }
    }

    // The quick actions. A list holds what the home screen draws, and a
    // cover can offer more than one list with only one on: two actions or
    // one, whichever are set, and none while the app cannot take them.
    CoverActionList {
        objectName: "twoActions"
        enabled: cover.actionsShown && cover.leftAction.kind !== ""
                 && cover.rightAction.kind !== ""

        CoverAction {
            objectName: "leftAction"
            iconSource: cover.actionIcon(cover.leftAction)
            onTriggered: cover.quickAction("left")
        }
        CoverAction {
            objectName: "rightAction"
            iconSource: cover.actionIcon(cover.rightAction)
            onTriggered: cover.quickAction("right")
        }
    }

    CoverActionList {
        objectName: "oneAction"
        enabled: cover.actionsShown && (cover.leftAction.kind === "")
                                       !== (cover.rightAction.kind === "")

        CoverAction {
            objectName: "onlyAction"
            readonly property string side: cover.leftAction.kind !== "" ? "left" : "right"
            iconSource: cover.actionIcon(side === "left" ? cover.leftAction
                                                         : cover.rightAction)
            onTriggered: cover.quickAction(side)
        }
    }
}
