import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../components"
import "../js/QuickActions.js" as QuickActions
import "../js/Media.js" as Media

/*
 * Cover while minimised: who is there, who wrote new.
 *
 * Staggered circle grid: everyone's avatar in grey once, empty circles for spare
 * cells; starts above top edge. Writers in ambience highlight in whole cells
 * nearest the top; then count pill (two cells wide, middle of second row). More
 * people than cells: pictures before initials.
 *
 * Every profile counts: one ChatList each; grid = everyone. Own lists: cover
 * outlives pages (minimise from anywhere, onboarding included). People come as
 * one JSON list per profile (`cover_people`), laid out in one pass.
 *
 * Foot: up to two quick actions (home screen draws them); grid fades into the
 * bottom strip, nothing over faces. Tap handling is the window's (piirit.qml).
 *
 * Call up: cover is the call's. Picture faint, shaded darkest at name and foot;
 * no picture: avatar over name. Name, time/status, mic/camera pills, one mic
 * toggle. No hang-up (mis-tap), no camera (stays let go while cover shows).
 * Ringing: no action, no pills. All inside side margins, above strip; smaller
 * covers scale down.
 *
 * Picture is the plain file: blurred face is no face on a cover. No shaders
 * anywhere on the cover (Avatar.qml).
 */
CoverBackground {
    id: cover

    // One list per profile. Repeater delegates must be Items: each list in an
    // empty one.
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

    /// Everyone across profiles, in each list's order.
    property var people: []
    /// Unread across profiles.
    property int unreadTotal: 0
    /// Can take quick action now (window says). Not before chat list, nor over a
    /// page that must not be left.
    property bool quickActionsAllowed: true

    /// Quick action tapped: `side` is "left" or "right" (as Settings).
    signal quickAction(string side)

    readonly property var leftAction: QuickActions.shown(Settings, "left")
    readonly property var rightAction: QuickActions.shown(Settings, "right")
    /// Home screen draws an action: one set, app can take it, no call up (call
    /// has own).
    readonly property bool actionsShown: cover.quickActionsAllowed && !cover.callUp
        && (cover.leftAction.kind !== "" || cover.rightAction.kind !== "")

    /// Icon picture as whole URL: home screen reads the file from outside app.
    function iconUrl(name) {
        return Qt.resolvedUrl("../" + QuickActions.iconFile(
            name, Theme.iconSizeSmall, QuickActions.isLight(Theme.primaryColor)))
    }

    /// Picture for an action; none for no action.
    function actionIcon(action) {
        if (action.kind === "") {
            return ""
        }
        return cover.iconUrl(QuickActions.iconName(action))
    }

    /// Window's call (CallCenter.qml), so cover can show it; null for none.
    property QtObject call: null

    /// Call up (ringing, placed, in progress) while call page on stack.
    readonly property bool callUp: cover.call !== null && cover.call.state !== ""
                                   && cover.call.state !== "ended"
    /// Past ringing: mic to switch, camera maybe let go. Placed calls: from
    /// start.
    readonly property bool callUnderWay: cover.callUp && cover.call.state !== "ringing"

    /// Other end's picture file; empty for none.
    readonly property string callPicturePath: cover.callUp ? "" + cover.call.peer_avatar : ""
    /// Picture drawn under call once loaded. Until then, or if never loads:
    /// plain cover.
    readonly property bool callPictured: cover.callUp && callPicture.status === Image.Ready

    /// Under shade, behind pills: black in dark ambience, white in light.
    readonly property color callGround: QuickActions.isLight(Theme.primaryColor)
                                        ? Qt.rgba(0, 0, 0, 1) : Qt.rgba(1, 1, 1, 1)

    /// Now, ticked once a second while call clock is seen.
    property real now: Date.now()

    /// Connected time as call page counts it (CallPage.qml): two digits each.
    function callElapsed() {
        if (!cover.callUp || cover.call.connected_at <= 0) {
            return ""
        }
        var seconds = Math.max(0, Math.floor(cover.now / 1000 - cover.call.connected_at))
        var parts = [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60]
        return parts.map(function (part) { return (part < 10 ? "0" : "") + part }).join(":")
    }

    /// Under name: connected time, or state until connected. Call page's words,
    /// in its translation context, so translated wherever it is.
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

    // Call clock only while seen: a timer would wake phone each second.
    Timer {
        id: callClock
        objectName: "callClock"
        interval: 1000
        repeat: true
        running: cover.looking && cover.callUp && cover.call.state === "connected"
        onTriggered: cover.now = Date.now()
        // Set now, not on first tick: cover looked at again must not show time
        // it went away.
        onRunningChanged: {
            if (callClock.running) {
                cover.now = Date.now()
            }
        }
    }

    /// Strip height home screen draws actions in. Measured, as vuo did: icon
    /// tops ~18% up.
    readonly property real actionStrip: Theme.itemSizeSmall
    /// Faces stop being seen whole: above strip if actions, else bottom edge.
    readonly property real floor: cover.actionsShown
                                  ? cover.height - cover.actionStrip
                                  : cover.height
    /// Band faces fade over with actions: twice strip height above bottom edge,
    /// to edge.
    readonly property real fadeFrom: cover.height - 2 * cover.actionStrip
    readonly property real fadeTo: cover.height
    /// Grid cells `{person, row, col, loud}`; `person` null for empty circle.
    property var cells: []

    /// Three across; every other row shifted half a cell and one more. Shifted
    /// row under first holds the pill (two middle cells).
    readonly property int columns: 3
    readonly property int cellSize: Math.floor(cover.width / cover.columns)
    /// Gap between circles, and circle and pill.
    readonly property int gap: Theme.paddingMedium
    readonly property int rowStep: Math.max(1, Math.round(cover.cellSize * 0.95))
    /// Grid starts this far above top edge: first row seen from waist down.
    readonly property int topCut: Math.round(cover.cellSize * 0.6)
    readonly property int rows: cover.cellSize > 0
                                ? Math.ceil((cover.height + cover.topCut) / cover.rowStep)
                                : 0

    /// Shifted row: one more cell, half a cell left.
    function shifted(row) {
        return row % 2 === 1
    }

    /// Cells per row.
    function across(row) {
        return cover.shifted(row) ? cover.columns + 1 : cover.columns
    }

    /// Cell position across grid.
    function cellX(row, col) {
        return col * cover.cellSize
               - (cover.shifted(row) ? cover.cellSize / 2 : 0)
    }

    /// Row position down grid.
    function cellY(row) {
        return row * cover.rowStep - cover.topCut
    }

    /// Pill's row and its two cells: middle pair of first shifted row.
    readonly property int pillRow: 1
    readonly property int pillFirstCol: cover.across(cover.pillRow) / 2 - 1
    /// Pill only when it has something to say.
    readonly property bool showPill: cover.unreadTotal > 0

    /// Cell under the pill.
    function underPill(row, col) {
        return row === cover.pillRow
               && (col === cover.pillFirstCol || col === cover.pillFirstCol + 1)
    }

    /// Visibility rank, smaller better. Excluded: half-cut, top-cut, fading.
    /// Rest: higher first, then centre out.
    function prominence(row, col) {
        var size = cover.cellSize
        var x = cover.cellX(row, col)
        var y = cover.cellY(row)
        var whole = x >= 0 && x + size <= cover.width
                    && y >= 0 && y + size <= cover.floor
        var fromMiddle = Math.abs((x + size / 2) - cover.width / 2) / size
        return (whole ? 0 : 100) + row * 2 + fromMiddle
    }

    /// Cover is what is looked at: minimised, home screen showing, display on.
    readonly property bool looking: cover.status === Cover.Active

    /// Grid is what is looked at: cover is, and no call. Laid out again once
    /// seen.
    readonly property bool gridSeen: cover.looking && !cover.callUp

    /// Changed while unseen: cells stale. True at start: nothing read yet.
    property bool stale: true

    /// Cover built. Size and status arrive one at a time; each would re-lay
    /// grid, so wait.
    property bool ready: false

    /// Re-read lists if anyone to read for, else mark stale.
    function gather() {
        if (!cover.ready || !cover.gridSeen) {
            cover.stale = true
            return
        }
        cover.stale = false
        cover.rebuild()
    }

    // Missed while away or in a call: done once on return, not per arrival.
    onGridSeenChanged: {
        if (cover.gridSeen && cover.stale) {
            cover.gather()
        }
    }

    /// Cell order: new first, then with picture, then rest; each in list order.
    function rank(person) {
        if (person.unread_count > 0) {
            return 0
        }
        return ("" + person.avatar_path).length > 0 ? 1 : 2
    }

    /// Who is there, how much unread, and what fills grid.
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
                // Keyed by profile too: two profiles can share a chat id.
                some[j].key = i + ":" + some[j].chat_id
                everyone.push(some[j])
            }
        }

        // Cells in drawn order, less the two the pill sits on.
        var slots = []
        for (var row = 0; row < cover.rows; row++) {
            for (var col = 0; col < cover.across(row); col++) {
                if (total > 0 && cover.underPill(row, col)) {
                    continue
                }
                slots.push({ row: row, col: col, person: null, loud: false })
            }
        }

        // Everyone once, in rank order. Chat id twice in one profile is one
        // person.
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

        // Best cells first; sorted apart from `slots` so grid keeps its order.
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
    // Cells worth having move with strip.
    onFloorChanged: cover.gather()

    // Grid fills cover past every edge: half a cell at sides, most of one at
    // top.
    Item {
        id: grid
        objectName: "avatarGrid"
        anchors.fill: parent
        clip: true
        visible: !cover.callUp

        // Count, not array: new array remakes every face (~20 Avatars); count
        // keeps them.
        Repeater {
            model: cover.cells.length

            Avatar {
                objectName: "gridCell"
                /// This face's cell. Empty while array shrinks before Repeater
                /// follows.
                readonly property var cell: cover.cells[index]
                                            || { person: null, row: 0, col: 0, loud: false }
                x: cover.cellX(cell.row, cell.col) + cover.gap / 2
                y: cover.cellY(cell.row) + cover.gap / 2
                width: cover.cellSize - cover.gap
                /// Whether anyone drawn here, or only the circle.
                readonly property bool filled: cell.person !== null
                initial: filled ? cell.person.name : ""
                ownColor: filled ? cell.person.color : ""
                picturePath: filled ? cell.person.avatar_path : ""
                // Phone's colours: writers in ambience highlight (unread badge
                // colour), others grey.
                monochrome: !cell.loud
                highlight: cell.loud
                opacity: cell.loud ? 1.0 : filled ? 0.6 : 0.35
                z: cell.loud ? 1 : 0
                // Action room: faces fade over band twice strip height
                // (QuickActions.fade). Each face fades itself: grid-level fade
                // layer drew later faces with the fade program (white squares).
                // See Avatar.qml.
                fadeFrom: cover.actionsShown ? (cover.fadeFrom - y) / Math.max(1, height) : 0
                fadeTo: cover.actionsShown ? (cover.fadeTo - y) / Math.max(1, height) : 0
            }
        }

        // Count in own cell, two wide: stretched circle sits among faces. Only
        // when something new. Outline, not disc: number on a highlight disc read
        // worse.
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
                // Shrunk to fit, not cut: "99+ new" in long-word languages must
                // read whole.
                fontSizeMode: Text.HorizontalFit
                minimumPixelSize: Theme.fontSizeTiny
                //: On the cover, in a pill among the avatars: how many
                //: messages are unread. %1 is the number, or "99+".
                text: qsTr("%1 new").arg(cover.unreadTotal > 99 ? "99+"
                                                                  : cover.unreadTotal)
            }
        }
    }

    // Call, in grid's place while up.
    Item {
        id: callView
        objectName: "callView"
        anchors.fill: parent
        clip: true
        visible: cover.callUp

        // Their face: larger than cover, faint, lightest shade. Read off main
        // thread.
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

        // Shade: dark top and foot, near clear between: name and action read
        // over faces.
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

        // Call text: inset, clear of strip (also while ringing, so nothing jumps).
        // Overflow drops avatar, then name's second line, then scales all down.
        Item {
            id: callBody
            objectName: "callBody"
            x: Theme.paddingMedium
            y: callBody.sized(Theme.paddingLarge)
            width: parent.width - 2 * Theme.paddingMedium
            height: Math.max(0, cover.height - cover.actionStrip
                                - callBody.sized(Theme.paddingSmall) - y)

            /// Theme sizes are for large cover. Smaller cover: all smaller, but
            /// side margins stay.
            readonly property real ratio: cover.width > 0 && cover.height > 0
                ? Math.min(1, cover.width / Theme.coverSizeLarge.width,
                           cover.height / Theme.coverSizeLarge.height)
                : 1
            /// Theme size for this cover.
            function sized(size) {
                return Math.round(size * callBody.ratio)
            }

            /// Name size and smallest it shrinks to for one line. Still largest
            /// words on cover.
            readonly property real nameSize: callBody.sized(Theme.fontSizeMedium)
            readonly property real nameLeast: callBody.sized(Theme.fontSizeSmall)
            /// Name width on one line at own size.
            readonly property real nameNatural: Math.max(1, nameRuler.implicitWidth)
            /// Name fits one line somewhere between the two sizes.
            readonly property bool nameOnOneLine: callBody.nameNatural * callBody.nameLeast
                                                  / callBody.nameSize <= callBody.width
            /// Drawn size: largest one line allows; least for two lines if one
            /// won't do.
            readonly property real nameFont: callBody.nameOnOneLine
                ? Math.max(callBody.nameLeast,
                           Math.min(callBody.nameSize,
                                    Math.floor(callBody.nameSize * callBody.width
                                               / callBody.nameNatural)))
                : callBody.nameLeast
            /// One line of name at that size.
            readonly property real nameLine: nameRuler.implicitHeight * callBody.nameFont
                                             / Math.max(1, callBody.nameSize)

            /// Room under name, pills inside it; nothing if none.
            readonly property real gap: callBody.sized(Theme.paddingMedium)
            readonly property real notesNeed: callNotes.implicitHeight > 0
                                              ? callBody.gap + callNotes.implicitHeight : 0
            /// Must fit whatever else is given up: one name line, time, pills.
            readonly property real least: callBody.nameLine + callHead.spacing
                                          + callTime.implicitHeight + callBody.notesNeed
            /// Name over two lines, when wanted and fits.
            readonly property int nameLines: !callBody.nameOnOneLine
                                             && callBody.height >= callBody.least + callBody.nameLine
                                             ? 2 : 1
            /// Avatar without picture: well short of cover width, only if fits
            /// over the rest.
            readonly property real avatarSize: Math.min(callBody.sized(Theme.iconSizeMedium),
                                                        Math.round(callBody.width * 0.3))
            readonly property real avatarGap: callBody.sized(Theme.paddingMedium)
            readonly property bool avatarShown: !cover.callPictured
                && callBody.height >= callBody.least
                                      + (callBody.nameLines - 1) * callBody.nameLine
                                      + callBody.avatarSize + callBody.avatarGap

            /// Height at own size, and how much smaller drawn if no room.
            readonly property real needed: callHead.height + callBody.notesNeed
            scale: callBody.needed > callBody.height && callBody.needed > 0
                   ? callBody.height / callBody.needed : 1
            transformOrigin: Item.Top

            // Name on one line at own size, never shown: its width gives the
            // size.
            Label {
                id: nameRuler
                visible: false
                font.pixelSize: callBody.nameSize
                textFormat: Text.PlainText
                text: cover.callUp ? cover.call.peer_name : ""
            }

            // Who, at top: what cover is for during call. Avatar over name when
            // no picture.
            Column {
                id: callHead
                objectName: "callHead"
                width: parent.width
                spacing: callBody.sized(Theme.paddingSmall)

                Avatar {
                    objectName: "callAvatar"
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: callBody.avatarShown
                    width: callBody.avatarSize
                    initial: cover.callUp ? cover.call.peer_name : ""
                    ownColor: cover.callUp ? cover.call.peer_color : ""
                }

                // More room avatar-to-name than name-to-time: those belong
                // together.
                Item {
                    visible: callBody.avatarShown
                    width: 1
                    height: callBody.avatarGap - callHead.spacing
                }

                Label {
                    id: callName
                    objectName: "callName"
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    maximumLineCount: callBody.nameLines
                    truncationMode: TruncationMode.Elide
                    font.pixelSize: callBody.nameFont
                    color: Theme.primaryColor
                    // Other end's own name for themselves.
                    textFormat: Text.PlainText
                    text: cover.callUp ? cover.call.peer_name : ""
                }

                Label {
                    id: callTime
                    objectName: "callTime"
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: callBody.sized(Theme.fontSizeExtraSmall)
                    color: Theme.highlightColor
                    textFormat: Text.PlainText
                    text: cover.callStatus()
                }
            }

            // Call facts, a pill each, middle of room between name and action.
            Column {
                id: callNotes
                objectName: "callNotes"
                readonly property real roomTop: callHead.height + callBody.gap
                // Body's own measure: all it needs when drawn smaller.
                readonly property real roomBottom: Math.max(callBody.height, callBody.needed)
                anchors.horizontalCenter: parent.horizontalCenter
                y: Math.round(callNotes.roomTop
                              + Math.max(0, (callNotes.roomBottom - callNotes.roomTop - height) / 2))
                spacing: callBody.sized(Theme.paddingSmall)

                // Undoable from here: loud.
                CallNote {
                    objectName: "micNote"
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: cover.callUnderWay && cover.call.muted
                    loud: true
                    ground: cover.callGround
                    maximumWidth: callBody.width
                    ratio: callBody.ratio
                    iconName: "icon-m-mic-mute"
                    //: On the cover during a call: the reader's microphone is
                    //: switched off, so the other end hears nothing.
                    text: qsTr("Microphone off")
                }

                // Wanted, and let go in background (cover seen, so it is). Back by
                // itself in front, so only said, no action.
                CallNote {
                    objectName: "cameraNote"
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: cover.callUnderWay && cover.call.camera
                    ground: cover.callGround
                    maximumWidth: callBody.width
                    ratio: callBody.ratio
                    iconName: "icon-m-video"
                    //: On the cover during a video call: the camera sends
                    //: nothing while the app is in the background, and comes
                    //: back on when the app is opened.
                    text: qsTr("Camera paused")
                }
            }
        }
    }

    /// Call shade at `strength`, in ground colour.
    function shade(strength) {
        var ground = cover.callGround
        return Qt.rgba(ground.r, ground.g, ground.b, strength)
    }

    // Call's one action, mic: off or on. Icon shows what tap does.
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

    // Quick actions: one or two as set; none if app can't take them. A cover may
    // hold several lists with only one enabled.
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
