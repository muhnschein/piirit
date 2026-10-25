import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../components"
import "../js/Emoji.js" as Emoji
import "../js/QuickActions.js" as QuickActions
import "../js/Media.js" as Media

/*
 * Cover while minimised: who is there, who wrote new.
 *
 * Staggered circle grid from above top edge: everyone once in grey, empty circles
 * spare. Writers in highlight, whole cells nearest top; count pill two cells wide,
 * mid second row. Overflow: pictures before initials.
 *
 * One ChatList per profile, own (cover outlives pages, onboarding too); people as
 * one JSON list per profile (`cover_people`), laid out in one pass.
 *
 * Foot: up to two quick actions (home screen draws); grid fades into strip. Taps:
 * window's (piirit.qml).
 *
 * Call up: cover is call's. Faint picture, darkest at name and foot; no picture:
 * avatar over name. Name, time/status, mic/camera pills, mic toggle. No hang-up
 * (mis-tap), no camera (let go while cover shows). Ringing: no action, no pills.
 * Inside side margins, above strip; small covers scale down.
 *
 * Plain picture file (blur = no face); no shaders on cover (Avatar.qml).
 */
CoverBackground {
    id: cover

    // One list per profile, each in an empty Item (Repeater delegate).
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

    /// Everyone, list order.
    property var people: []
    /// Unread across profiles.
    property int unreadTotal: 0
    /// Quick action takeable now (window says): not before chat list or over
    /// unleavable page.
    property bool quickActionsAllowed: true

    /// Quick action tapped: `side` "left"/"right" (as Settings).
    signal quickAction(string side)

    readonly property var leftAction: QuickActions.shown(Settings, "left")
    readonly property var rightAction: QuickActions.shown(Settings, "right")
    /// Actions drawn: one set, allowed, no call up.
    readonly property bool actionsShown: cover.quickActionsAllowed && !cover.callUp
        && (cover.leftAction.kind !== "" || cover.rightAction.kind !== "")

    /// Icon as whole URL: home screen reads it from outside app.
    function iconUrl(name) {
        return Qt.resolvedUrl("../" + QuickActions.iconFile(
            name, Theme.iconSizeSmall, QuickActions.isLight(Theme.primaryColor)))
    }

    /// Action icon; "" for none.
    function actionIcon(action) {
        if (action.kind === "") {
            return ""
        }
        return cover.iconUrl(QuickActions.iconName(action))
    }

    /// Window's call (CallCenter.qml); null for none.
    property QtObject call: null

    /// Call up (ringing, placed, in progress), page on stack.
    readonly property bool callUp: cover.call !== null && cover.call.state !== ""
                                   && cover.call.state !== "ended"
    /// Past ringing (placed: from start): mic switchable.
    readonly property bool callUnderWay: cover.callUp && cover.call.state !== "ringing"

    /// Other end's picture file; empty for none.
    readonly property string callPicturePath: cover.callUp ? "" + cover.call.peer_avatar : ""
    /// Picture under call once loaded; else plain.
    readonly property bool callPictured: cover.callUp && callPicture.status === Image.Ready

    /// Shade ground: black in dark ambience, white in light.
    readonly property color callGround: QuickActions.isLight(Theme.primaryColor)
                                        ? Qt.rgba(0, 0, 0, 1) : Qt.rgba(1, 1, 1, 1)

    /// Now, ticked each second while call clock seen.
    property real now: Date.now()

    /// Connected time as CallPage.qml: two digits each.
    function callElapsed() {
        if (!cover.callUp || cover.call.connected_at <= 0) {
            return ""
        }
        var seconds = Math.max(0, Math.floor(cover.now / 1000 - cover.call.connected_at))
        var parts = [Math.floor(seconds / 3600), Math.floor(seconds / 60) % 60, seconds % 60]
        return parts.map(function (part) { return (part < 10 ? "0" : "") + part }).join(":")
    }

    /// Under name: connected time, or state. Call page's words and context, so
    /// translated wherever it is.
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

    // Clock only while seen: timer wakes phone each second.
    Timer {
        id: callClock
        objectName: "callClock"
        interval: 1000
        repeat: true
        running: cover.looking && cover.callUp && cover.call.state === "connected"
        onTriggered: cover.now = Date.now()
        // Set now: else shows time cover went away.
        onRunningChanged: {
            if (callClock.running) {
                cover.now = Date.now()
            }
        }
    }

    /// Action strip height, measured (as vuo): icon tops ~18% up.
    readonly property real actionStrip: Theme.itemSizeSmall
    /// Where faces stop being whole: strip top if actions, else bottom.
    readonly property real floor: cover.actionsShown
                                  ? cover.height - cover.actionStrip
                                  : cover.height
    /// Fade band with actions: twice strip height.
    readonly property real fadeFrom: cover.height - 2 * cover.actionStrip
    readonly property real fadeTo: cover.height
    /// Grid cells `{person, row, col, loud}`; `person` null for empty circle.
    property var cells: []

    /// Three across; alternate rows shifted half a cell, one more. First shifted
    /// row holds pill.
    readonly property int columns: 3
    readonly property int cellSize: Math.floor(cover.width / cover.columns)
    /// Gap between circles, and circle and pill.
    readonly property int gap: Theme.paddingMedium
    readonly property int rowStep: Math.max(1, Math.round(cover.cellSize * 0.95))
    /// Grid offset above top: first row seen from waist down.
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

    /// Visibility rank, smaller better; half-, top-cut, fading excluded. Then
    /// higher first, centre out.
    function prominence(row, col) {
        var size = cover.cellSize
        var x = cover.cellX(row, col)
        var y = cover.cellY(row)
        var whole = x >= 0 && x + size <= cover.width
                    && y >= 0 && y + size <= cover.floor
        var fromMiddle = Math.abs((x + size / 2) - cover.width / 2) / size
        return (whole ? 0 : 100) + row * 2 + fromMiddle
    }

    /// Cover seen: minimised, home screen up, display on.
    readonly property bool looking: cover.status === Cover.Active

    /// Grid seen: cover seen, no call. Re-laid once seen.
    readonly property bool gridSeen: cover.looking && !cover.callUp

    /// Changed while unseen; true at start.
    property bool stale: true

    /// Cover built: size and status come one by one, each would re-lay grid.
    property bool ready: false

    /// Re-read if seen, else mark stale.
    function gather() {
        if (!cover.ready || !cover.gridSeen) {
            cover.stale = true
            return
        }
        cover.stale = false
        cover.rebuild()
    }

    // Missed while away/in call: once on return.
    onGridSeenChanged: {
        if (cover.gridSeen && cover.stale) {
            cover.gather()
        }
    }

    /// Cell order: new, then with picture, then rest; list order within.
    function rank(person) {
        if (person.unread_count > 0) {
            return 0
        }
        return ("" + person.avatar_path).length > 0 ? 1 : 2
    }

    /// People, unread, grid.
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

        // Cells in drawn order, less pill's two.
        var slots = []
        for (var row = 0; row < cover.rows; row++) {
            for (var col = 0; col < cover.across(row); col++) {
                if (total > 0 && cover.underPill(row, col)) {
                    continue
                }
                slots.push({ row: row, col: col, person: null, loud: false })
            }
        }

        // Everyone once, rank order; same chat id in a profile = one person.
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

        // Best cells first; separate from `slots`, which keeps grid order.
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

    // Grid past edges: half a cell at sides, most of one at top.
    Item {
        id: grid
        objectName: "avatarGrid"
        anchors.fill: parent
        clip: true
        visible: !cover.callUp

        // Count, not array: new array remakes ~20 Avatars.
        Repeater {
            model: cover.cells.length

            Avatar {
                objectName: "gridCell"
                /// Face's cell; empty while array shrinks before Repeater.
                readonly property var cell: cover.cells[index]
                                            || { person: null, row: 0, col: 0, loud: false }
                x: cover.cellX(cell.row, cell.col) + cover.gap / 2
                y: cover.cellY(cell.row) + cover.gap / 2
                width: cover.cellSize - cover.gap
                /// Someone drawn here, not just circle.
                readonly property bool filled: cell.person !== null
                initial: filled ? cell.person.name : ""
                ownColor: filled ? cell.person.color : ""
                picturePath: filled ? cell.person.avatar_path : ""
                // Writers in highlight (unread badge colour), others grey.
                monochrome: !cell.loud
                highlight: cell.loud
                opacity: cell.loud ? 1.0 : filled ? 0.6 : 0.35
                z: cell.loud ? 1 : 0
                // Faces fade over band (QuickActions.fade), each itself: grid
                // layer drew later faces as white squares. See Avatar.qml.
                fadeFrom: cover.actionsShown ? (cover.fadeFrom - y) / Math.max(1, height) : 0
                fadeTo: cover.actionsShown ? (cover.fadeTo - y) / Math.max(1, height) : 0
            }
        }

        // Count, two cells wide, only when new. Outline: number on disc read
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
                // Shrunk, not cut: long-word "99+ new" reads whole.
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

        // Their face: larger than cover, faint; loaded off main thread.
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

        // Shade: dark top and foot so name and action read over face.
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

        // Call text clear of strip (also ringing: no jump). Overflow drops avatar,
        // name's second line, then scales down.
        Item {
            id: callBody
            objectName: "callBody"
            x: Theme.paddingMedium
            y: callBody.sized(Theme.paddingLarge)
            width: parent.width - 2 * Theme.paddingMedium
            height: Math.max(0, cover.height - cover.actionStrip
                                - callBody.sized(Theme.paddingSmall) - y)

            /// Theme sizes fit large cover; smaller cover scales, margins stay.
            readonly property real ratio: cover.width > 0 && cover.height > 0
                ? Math.min(1, cover.width / Theme.coverSizeLarge.width,
                           cover.height / Theme.coverSizeLarge.height)
                : 1
            /// Theme size for this cover.
            function sized(size) {
                return Math.round(size * callBody.ratio)
            }

            /// Name size and one-line minimum (still largest text).
            readonly property real nameSize: callBody.sized(Theme.fontSizeMedium)
            readonly property real nameLeast: callBody.sized(Theme.fontSizeSmall)
            /// Name width on one line at own size.
            readonly property real nameNatural: Math.max(1, nameRuler.implicitWidth)
            /// One line fits between the two sizes.
            readonly property bool nameOnOneLine: callBody.nameNatural * callBody.nameLeast
                                                  / callBody.nameSize <= callBody.width
            /// Drawn size: largest one-line; else minimum, two lines.
            readonly property real nameFont: callBody.nameOnOneLine
                ? Math.max(callBody.nameLeast,
                           Math.min(callBody.nameSize,
                                    Math.floor(callBody.nameSize * callBody.width
                                               / callBody.nameNatural)))
                : callBody.nameLeast
            /// One line of name at that size.
            readonly property real nameLine: nameRuler.implicitHeight * callBody.nameFont
                                             / Math.max(1, callBody.nameSize)

            /// Pill room under name; none if no pills.
            readonly property real gap: callBody.sized(Theme.paddingMedium)
            readonly property real notesNeed: callNotes.implicitHeight > 0
                                              ? callBody.gap + callNotes.implicitHeight : 0
            /// Always kept: one name line, time, pills.
            readonly property real least: callBody.nameLine + callHead.spacing
                                          + callTime.implicitHeight + callBody.notesNeed
            /// Name over two lines, when wanted and fits.
            readonly property int nameLines: !callBody.nameOnOneLine
                                             && callBody.height >= callBody.least + callBody.nameLine
                                             ? 2 : 1
            /// Avatar without picture, well under cover width, if room.
            readonly property real avatarSize: Math.min(callBody.sized(Theme.iconSizeMedium),
                                                        Math.round(callBody.width * 0.3))
            readonly property real avatarGap: callBody.sized(Theme.paddingMedium)
            readonly property bool avatarShown: !cover.callPictured
                && callBody.height >= callBody.least
                                      + (callBody.nameLines - 1) * callBody.nameLine
                                      + callBody.avatarSize + callBody.avatarGap

            /// Height at own size; shrink factor if no room.
            readonly property real needed: callHead.height + callBody.notesNeed
            scale: callBody.needed > callBody.height && callBody.needed > 0
                   ? callBody.height / callBody.needed : 1
            transformOrigin: Item.Top

            /// Their name; emoji as Twemoji (Emoji.js), "" when none.
            readonly property string namePlain: cover.callUp ? cover.call.peer_name : ""
            readonly property string namePictured: Emoji.inText(callBody.namePlain, false,
                                                                callBody.nameSize,
                                                                Emoji.nameTint(Theme.primaryColor))

            // Hidden one-line name: width sets size.
            Label {
                id: nameRuler
                visible: false
                font.pixelSize: callBody.nameSize
                textFormat: callBody.namePictured.length > 0 ? Text.StyledText : Text.PlainText
                text: callBody.namePictured.length > 0 ? callBody.namePictured
                                                       : callBody.namePlain
            }

            // Who, at top; avatar over name when no picture.
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

                // More gap avatar-name than name-time (those belong together).
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
                    // Cut, not elided, with pictures: elided last line misplaces them
                    // (MessageDelegate body).
                    readonly property string pictured: Emoji.inText(callBody.namePlain, false,
                                                                    callBody.nameFont,
                                                                    Emoji.nameTint(color))
                    truncationMode: callName.pictured.length > 0 ? TruncationMode.None
                                                                 : TruncationMode.Elide
                    font.pixelSize: callBody.nameFont
                    color: Theme.primaryColor
                    // Their self-chosen name.
                    textFormat: callName.pictured.length > 0 ? Text.StyledText : Text.PlainText
                    text: callName.pictured.length > 0 ? callName.pictured : callBody.namePlain
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

            // Call facts as pills, centred between name and action.
            Column {
                id: callNotes
                objectName: "callNotes"
                readonly property real roomTop: callHead.height + callBody.gap
                // Natural size, before scale.
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

                // Wanted, let go in background (cover seen); back by itself: no
                // action.
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

    // Call action: mic toggle; icon shows tap's effect.
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

    // Quick actions, one or two; none if not allowed. Cover may hold several
    // lists, one enabled.
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
