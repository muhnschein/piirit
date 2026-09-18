import QtQuick 2.0
import Sailfish.Silica 1.0
import Piiri 1.0
import "../components"

/*
 * What the cover has to say while the app is minimised: who is there,
 * and whether any of them has said something new.
 *
 * The whole cover is a staggered grid of everyone's avatar, in grey,
 * the few repeated to fill it; whoever has written is drawn in the
 * ambience's own highlight colour, in the cells that are seen whole and
 * nearest the top. The count sits in the grid rather than over it: a
 * pill the width of two cells, in the middle of the second row, saying
 * how many messages are new -- highlighted when the answer is not zero,
 * grey like the faces around it when it is. Nobody yet -- no chat but
 * the one with oneself and the core's own -- and the cover says so in a
 * line instead of the faces.
 *
 * Every profile counts: one ChatList per configured profile, so the
 * number is every unread message on the phone and the grid is everyone
 * the reader talks to, whichever profile they are under. The lists are
 * the cover's own rather than the chat list page's, because a cover
 * outlives any page -- the app can be minimised from anywhere, including
 * onboarding. The people come out of each list as one JSON list
 * (`cover_people`): the grid is laid out in a pass over them, which a
 * view over the rows could not repeat to fill.
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
        core.refresh_accounts()
        cover.gather()
    }

    /// Everyone, across every profile, in each list's order.
    property var people: []
    /// Unread messages across every profile.
    property int unreadTotal: 0
    /// What the grid draws: `{person, row, col, loud}` per cell, the
    /// people repeated to fill it and `loud` on the one cell each person
    /// with something new is given.
    property var cells: []

    /// The grid's shape: three across, with every other row shifted half
    /// a cell and holding one more, cut off at both edges -- so the rows
    /// nest, and the grid reads as a field of faces rather than a table.
    /// The first row is a whole one, so the shifted row under it is where
    /// the pill goes: its two middle cells are whole and centred.
    readonly property int columns: 3
    readonly property int cellSize: Math.floor(cover.width / cover.columns)
    readonly property int rowStep: Math.max(1, Math.round(cover.cellSize * 0.9))
    readonly property int rows: cover.cellSize > 0
                                ? Math.ceil(cover.height / cover.rowStep)
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

    /// The row the pill is in, and the two cells it takes: the middle
    /// pair of the first shifted row, which the shift puts on either
    /// side of the centre line.
    readonly property int pillRow: 1
    readonly property int pillFirstCol: cover.across(cover.pillRow) / 2 - 1

    /// Whether a cell is one of the two the pill is drawn over.
    function underPill(row, col) {
        return row === cover.pillRow
               && (col === cover.pillFirstCol || col === cover.pillFirstCol + 1)
    }

    /// How good a cell is to be seen in, smaller being better.
    ///
    /// A cover is glanced at, so a face that matters cannot be one of
    /// the halves the shifted rows leave hanging off an edge, or the
    /// part-row the bottom cuts through. What is left is ranked by how
    /// far down the cover it is and then by how far out from the middle,
    /// which is the order an eye takes them in.
    function prominence(row, col) {
        var size = cover.cellSize
        var x = cover.cellX(row, col)
        var whole = x >= 0 && x + size <= cover.width
                    && row * cover.rowStep + size <= cover.height
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

    /// Read the lists again if there is anyone to read them for, and
    /// otherwise remember that they want reading.
    ///
    /// Called on every change to any list and whenever the shape changes,
    /// so the cells are the right number by the time they are drawn.
    function gather() {
        if (!cover.looking) {
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
        // two the pill sits on.
        var slots = []
        for (var row = 0; row < cover.rows && everyone.length > 0; row++) {
            for (var col = 0; col < cover.across(row); col++) {
                if (cover.underPill(row, col)) {
                    continue
                }
                slots.push({ row: row, col: col, person: null, loud: false })
            }
        }

        // Whoever has something new, once each and in the lists' order.
        var loud = []
        var lit = {}
        for (var p = 0; p < everyone.length; p++) {
            if (everyone[p].unread_count > 0 && !lit[everyone[p].key]) {
                lit[everyone[p].key] = true
                loud.push(everyone[p])
            }
        }

        // They get the cells worth having. Sorted apart from `slots` so
        // the grid is still drawn in its own order.
        var best = slots.slice()
        best.sort(function (one, other) {
            return cover.prominence(one.row, one.col)
                   - cover.prominence(other.row, other.col)
        })
        for (var l = 0; l < loud.length && l < best.length; l++) {
            best[l].person = loud[l]
            best[l].loud = true
        }

        // Everyone fills what is left, repeated as far as it goes --
        // including whoever is already lit somewhere else, drawn grey
        // there as anyone else is.
        var made = []
        var next = 0
        for (var s = 0; s < slots.length; s++) {
            var slot = slots[s]
            if (!slot.person) {
                slot.person = everyone[next % everyone.length]
                next++
            }
            made.push({ person: slot.person, row: slot.row, col: slot.col,
                        loud: slot.loud })
        }

        cover.people = everyone
        cover.unreadTotal = total
        cover.cells = made
    }
    onRowsChanged: cover.gather()
    onWidthChanged: cover.gather()

    // Everyone, filling the cover. The shifted rows run past both edges
    // by half a cell, which the clip takes care of.
    Item {
        id: grid
        objectName: "avatarGrid"
        anchors.fill: parent
        clip: true

        Repeater {
            model: cover.cells

            Avatar {
                objectName: "gridCell"
                x: cover.cellX(modelData.row, modelData.col)
                y: modelData.row * cover.rowStep
                width: cover.cellSize - Theme.paddingSmall
                initial: modelData.person.name
                ownColor: modelData.person.color
                picturePath: modelData.person.avatar_path
                // Nobody is drawn in their own colours here. A cover is
                // the phone's, not the app's: whoever has written is the
                // ambience's highlight, the same colour the unread badge
                // in the chat list wears, and everyone else is grey.
                monochrome: !modelData.loud
                highlight: modelData.loud
                opacity: modelData.loud ? 1.0 : 0.6
                // Over its neighbours rather than under them: the rows
                // nest, so a cell's bottom edge is drawn on by the row
                // below it, and a lit face should not be the one that
                // loses a sliver.
                z: modelData.loud ? 1 : 0
            }
        }

        // The count, in a cell of its own two cells wide: a circle
        // stretched sideways, so it sits among the faces as one of them
        // rather than over them. Always drawn -- a zero says as much as
        // a count -- and in the highlight only when there is something
        // new, the way a face is.
        Rectangle {
            id: pill
            objectName: "unreadPill"
            x: cover.cellX(cover.pillRow, cover.pillFirstCol)
            y: cover.pillRow * cover.rowStep
            width: 2 * cover.cellSize - Theme.paddingSmall
            height: cover.cellSize - Theme.paddingSmall
            radius: height / 2
            /// Whether there is something new, which is what colours it.
            readonly property bool highlight: cover.unreadTotal > 0
            color: highlight ? Theme.highlightColor
                             : Theme.rgba(Theme.primaryColor, 0.25)
            opacity: highlight ? 1.0 : 0.6
            // Above the faces for the same reason a lit one is: the row
            // under it draws over its bottom edge otherwise.
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
                color: Theme.primaryColor
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

    // Nobody yet: say so, in a line that wraps rather than runs off the
    // cover in a language where it is longer. Under the pill, which is
    // still there to say that nothing is new.
    Label {
        objectName: "emptyLabel"
        anchors {
            top: pill.bottom
            left: parent.left
            right: parent.right
            topMargin: Theme.paddingLarge
            leftMargin: Theme.paddingLarge
            rightMargin: Theme.paddingLarge
        }
        visible: cover.people.length === 0
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        font.pixelSize: Theme.fontSizeSmall
        color: Theme.secondaryColor
        text: qsTr("No messages")
    }
}
