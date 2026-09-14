import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * The ways on from a page that asks something: one tile per answer, side
 * by side, an icon over the words for it.
 *
 * The same thing the media row on a contact's page is (MediaKinds.qml),
 * and drawn the same way, because these are doors as well. A push button
 * is the right shape for "do this to what is already on the screen"; the
 * onboarding pages have nothing on the screen yet, and what they offer
 * is two places to go. Two buttons of the same size, one under the
 * other, say only that there are two of something -- an icon over each
 * says which is which before the words are read, and the reader who
 * needs that most is the one on the first screen of an app they have
 * never opened.
 *
 * Every icon here is the theme's own, in the theme's colours, so the
 * tiles wear whatever ambience the phone wears. They are asked for by
 * names this app has seen on a device (see AppMark for what an icon that
 * is not there looks like): a reader on a tile they cannot see is a
 * reader who cannot start.
 *
 * Laid out by bindings rather than a Row, for the reason MediaKinds
 * gives: a positioner sizes itself in a polish pass, which never runs
 * headlessly, so a row built from one measures as nothing in a test.
 *
 * Nothing is opened here. The page that owns the pageStack decides what
 * an answer means, which keeps this loadable, and testable, on its own.
 */
Item {
    id: root

    /// The answers, in tile order. Each one is an object:
    ///
    /// - `name`: what `chosen` hands back, and what the tile is called --
    ///   a tile is `<name>Tile`, which is how a test finds it.
    /// - `icon`: the theme icon over the words, by name.
    /// - `text`: the words themselves.
    /// - `hint`: a quieter line under them, or left out for none.
    /// - `enabled`: false to grey the tile out and ignore taps on it.
    ///   Left out means on.
    property var choices: []

    /// How far in from its own edges the row starts: its own margins on a
    /// page that lays nothing out, and none inside a column that is
    /// already inset by them.
    property real sideMargin: Theme.horizontalPageMargin

    /// Whether the tiles stand one under another rather than side by
    /// side. Two tiles share a screen comfortably; three leave each of
    /// them a third of it, which is not room for a line of words with a
    /// second line under it, so the page that asks three things at once
    /// stacks them instead.
    ///
    /// Stacked, a tile is a row rather than a tile: the icon at the left
    /// where an avatar would be, the words beside it and ranged left,
    /// which is how every other row in this app reads
    /// (components/ContactRow.qml).
    property bool stacked: false

    /// What is kept between stacked tiles, so that two of them read as
    /// two rather than as one tall block.
    property real gap: Theme.paddingMedium

    /// The reader picked one, by `name`.
    signal chosen(string name)

    /// How wide one tile is: the whole row stacked, and what the margins
    /// leave shared out otherwise.
    readonly property real tileWidth: root.choices.length > 0
        ? (root.stacked
           ? root.width - 2 * root.sideMargin
           : (root.width - 2 * root.sideMargin) / root.choices.length)
        : 0

    /// How tall every tile is: as tall as the one that needs most room,
    /// so the row reads as a row rather than as blocks of two heights.
    /// A tile says what it needs and this follows the tallest.
    property real tileHeight: 0

    /// Take the tallest tile's word for how tall a tile is.
    function measure() {
        var tallest = 0
        for (var i = 0; i < tiles.count; i++) {
            var tile = tiles.itemAt(i)
            if (tile) {
                tallest = Math.max(tallest, tile.needed)
            }
        }
        root.tileHeight = tallest
    }

    width: parent ? parent.width : 0
    height: root.stacked
            ? root.choices.length * root.tileHeight
              + Math.max(0, root.choices.length - 1) * root.gap
            : root.tileHeight

    Repeater {
        id: tiles
        model: root.choices
        // A tile that is not built yet has nothing to say about its
        // height; this is where the last of them says it.
        onItemAdded: root.measure()
        onItemRemoved: root.measure()

        BackgroundItem {
            id: tile
            objectName: modelData.name + "Tile"
            x: root.sideMargin + (root.stacked ? 0 : index * root.tileWidth)
            y: root.stacked ? index * (root.tileHeight + root.gap) : 0
            width: root.tileWidth
            height: root.tileHeight
            // `undefined` is a choice that never mentioned it, which is
            // every choice that is always on.
            enabled: modelData.enabled === undefined || modelData.enabled

            /// The bottom of the words, whichever of them is the last.
            readonly property real wordsEnd: hint.visible
                ? hint.y + hint.height : caption.y + caption.height

            /// What this tile's own contents come to, top to bottom: the
            /// words under the icon side by side, and whichever of the
            /// two runs lower when they are beside each other. Read by
            /// the row, which draws every tile at the largest.
            readonly property real needed:
                Math.max(icon.y + icon.height, tile.wordsEnd) + Theme.paddingMedium
            onNeededChanged: root.measure()

            // Lit under a thumb, the way an IconButton is; grey where the
            // tile is not to be tapped, which is how Silica says so.
            readonly property color tint: !tile.enabled
                ? Theme.secondaryColor
                : tile.highlighted ? Theme.highlightColor : Theme.primaryColor

            // Placed rather than anchored: the two ways a tile is laid
            // out want different edges, and a binding reads better than
            // two sets of anchors that undo each other.
            Image {
                id: icon
                objectName: "tileIcon"
                // At the left of the row it is the mark on, the way an
                // avatar is on a contact's row; over the words when the
                // tiles stand side by side.
                x: root.stacked ? Theme.paddingMedium
                                : (tile.width - icon.width) / 2
                y: Theme.paddingMedium
                width: Theme.iconSizeMedium
                height: width
                source: "image://theme/" + modelData.icon + "?" + tile.tint
            }

            Label {
                id: caption
                objectName: "tileLabel"
                x: root.stacked ? icon.x + icon.width + Theme.paddingMedium
                                : Theme.paddingSmall
                // Beside the icon: at the top when there is a second
                // line to come, and level with the middle of the icon
                // when the one line is all there is (ContactRow.qml does
                // the same with a name and an address).
                y: root.stacked
                   ? (hint.visible ? Theme.paddingMedium
                                   : icon.y + (icon.height - caption.height) / 2)
                   : icon.y + icon.height + Theme.paddingSmall
                width: tile.width - caption.x
                       - (root.stacked ? Theme.paddingMedium : Theme.paddingSmall)
                horizontalAlignment: root.stacked ? Text.AlignLeft
                                                  : Text.AlignHCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: tile.tint
                text: modelData.text
            }

            Label {
                id: hint
                objectName: "tileHint"
                visible: hint.text.length > 0
                x: caption.x
                // Straight under the line above it where the two are a
                // block beside an icon, and a gap below the words where
                // they are a column under one.
                y: caption.y + caption.height
                   + (root.stacked ? 0 : Theme.paddingSmall)
                width: caption.width
                horizontalAlignment: caption.horizontalAlignment
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: modelData.hint === undefined ? "" : modelData.hint
            }

            onClicked: root.chosen(modelData.name)
        }
    }
}
