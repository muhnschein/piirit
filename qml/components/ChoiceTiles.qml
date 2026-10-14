import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Tiles for a page that asks something: one per answer, icon over words.
 * Same look as MediaKinds.qml's media row: these are doors too. Push button
 * fits acting on screen content; onboarding has none, just places to go. Icon
 * says which is which before reading: matters most on an app's first screen.
 *
 * Icons: theme's own (follow ambience), asked only by names seen on a device
 * (see AppMark: a missing icon is a tile nobody sees, a reader who can't start).
 * First-screen profile tile can't risk even that: app-drawn AccountMark.qml.
 *
 * Bindings, not Row: positioner sizes in polish pass, never runs headless.
 * Opens nothing; owning page decides what an answer means.
 */
Item {
    id: root

    /// Answers, in tile order. Each an object:
    /// - `name`: handed back by `chosen`; tile is `<name>Tile`.
    /// - `icon`: theme icon name, over the words.
    /// - `mark`: app-drawn mark instead of icon, for tiles that must show on every
    ///   phone (`account`: AccountMark.qml). Given, `icon` is not read.
    /// - `text`: the words. `hint`: quieter line under them, optional.
    /// - `enabled`: false greys out, ignores taps. Default on.
    property var choices: []

    /// Inset from edges: margins on bare page, none inside inset column.
    property real sideMargin: Theme.horizontalPageMargin

    /// Stacked: three side by side leave no room for two-line words.
    /// Stacked tile is a row: icon left, words ranged left (as ContactRow.qml).
    property bool stacked: false

    /// Gap between stacked tiles.
    property real gap: Theme.paddingMedium

    /// Picked one, by `name`.
    signal chosen(string name)

    /// Tile width: whole row stacked, else share after margins.
    readonly property real tileWidth: root.choices.length > 0
        ? (root.stacked
           ? root.width - 2 * root.sideMargin
           : (root.width - 2 * root.sideMargin) / root.choices.length)
        : 0

    /// Tile height: tallest tile's, so row reads as one.
    property real tileHeight: 0

    /// Tallest tile's height.
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
        // Unbuilt tile: no height yet; last one to build reports.
        onItemAdded: root.measure()
        onItemRemoved: root.measure()

        BackgroundItem {
            id: tile
            objectName: modelData.name + "Tile"
            x: root.sideMargin + (root.stacked ? 0 : index * root.tileWidth)
            y: root.stacked ? index * (root.tileHeight + root.gap) : 0
            width: root.tileWidth
            height: root.tileHeight
            // `undefined` = never mentioned: always on.
            enabled: modelData.enabled === undefined || modelData.enabled

            /// Bottom of words.
            readonly property real wordsEnd: hint.visible
                ? hint.y + hint.height : caption.y + caption.height

            /// Tile content height: icon then words under it; stacked, whichever of
            /// icon and words runs lower. Row draws all at the largest.
            readonly property real needed:
                Math.max(icon.y + icon.height, tile.wordsEnd) + Theme.paddingMedium
            onNeededChanged: root.measure()

            // Lit under thumb (as IconButton); grey when untappable.
            readonly property color tint: !tile.enabled
                ? Theme.secondaryColor
                : tile.highlighted ? Theme.highlightColor : Theme.primaryColor

            // Placed, not anchored: layouts want different edges.
            Image {
                id: icon
                objectName: "tileIcon"
                // Left of row, as avatar on contact row; over words when side by side.
                x: root.stacked ? Theme.paddingMedium
                                : (tile.width - icon.width) / 2
                y: Theme.paddingMedium
                width: Theme.iconSizeMedium
                height: width
                // No theme asked where mark drawn.
                source: modelData.mark === undefined
                        ? "image://theme/" + modelData.icon + "?" + tile.tint
                        : ""
            }

            // Drawn mark, in icon's place and colour.
            AccountMark {
                objectName: "tileMark"
                visible: modelData.mark === "account"
                anchors.fill: icon
                size: icon.width
                color: tile.tint
            }

            Label {
                id: caption
                objectName: "tileLabel"
                x: root.stacked ? icon.x + icon.width + Theme.paddingMedium
                                : Theme.paddingSmall
                // Beside icon: top if second line, else middle (as ContactRow.qml).
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
                // Under line above if block beside icon; gap below if column.
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
