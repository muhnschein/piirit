import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../components"
import "../js/QuickActions.js" as QuickActions

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
 * At its foot, up to two quick actions the reader chose in the settings:
 * a chat, the search, this profile's QR code, or the scanner. The home
 * screen draws them, in the strip along the bottom edge, and while they
 * are there the grid sinks away into that strip rather than running
 * under the icons -- the faces fade out towards the bottom, nothing is
 * laid over them. What a tap does is the window's (piirit.qml); the cover
 * only says which one it was.
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

    readonly property var leftAction: QuickActions.read(Settings, "left")
    readonly property var rightAction: QuickActions.read(Settings, "right")
    /// Whether the home screen is drawing any actions: one set, and the
    /// app able to take it.
    readonly property bool actionsShown: cover.quickActionsAllowed
        && (cover.leftAction.kind !== "" || cover.rightAction.kind !== "")

    /// The picture for an action, as a whole URL: the home screen reads
    /// the file itself, from outside the app. None for no action.
    function actionIcon(action) {
        if (action.kind === "") {
            return ""
        }
        return Qt.resolvedUrl("../" + QuickActions.iconFile(
            QuickActions.iconName(action), Theme.iconSizeSmall,
            QuickActions.isLight(Theme.primaryColor)))
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
        if (!cover.ready || !cover.looking) {
            cover.stale = true
            return
        }
        cover.stale = false
        cover.rebuild()
    }

    // Whatever was missed while the cover was away, done once on the way
    // back rather than once per arrival while it was gone.
    onLookingChanged: {
        if (cover.looking && cover.stale) {
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

        // The room for the actions, made by the grid rather than over it:
        // towards the bottom edge the faces run out, over a band twice the
        // strip's height, so the icons sit on the cover's own ground with
        // the faces nearly gone around them. Eased rather than straight,
        // as vuo's cover eases its texture away: most of the way down the
        // band the faces keep their strength, and they give up the rest
        // near the bottom -- a straight ramp reads as a wash laid over
        // them. Off while there are no actions, so the faces run whole to
        // the edge.
        layer.enabled: cover.actionsShown
        layer.effect: ShaderEffect {
            objectName: "actionFade"
            property real fadeFrom: grid.height - 2 * cover.actionStrip
            property real fadeTo: grid.height
            property real gridHeight: Math.max(1, grid.height)

            // highp for the ramp, which runs over a good part of the cover
            // and would band in steps at lowp.
            fragmentShader: "
                varying highp vec2 qt_TexCoord0;
                uniform sampler2D source;
                uniform highp float fadeFrom;
                uniform highp float fadeTo;
                uniform highp float gridHeight;
                uniform lowp float qt_Opacity;

                void main() {
                    highp float y = qt_TexCoord0.y * gridHeight;
                    highp float sink = clamp((fadeTo - y) / max(1.0, fadeTo - fadeFrom),
                                             0.0, 1.0);
                    // Premultiplied, so the whole colour goes with it.
                    gl_FragColor = texture2D(source, qt_TexCoord0) * (sink * sink)
                                   * qt_Opacity;
                }"
        }

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
