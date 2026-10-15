import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Tiles for a page that asks: one per answer, icon over words. Looks like MediaKinds.qml's
 * media row: doors too. Push button suits acting on screen content; onboarding just has
 * places to go. Icon says which before reading: matters most on first screen.
 *
 * Icons: theme's (follow ambience), only names seen on a device (see AppMark: missing icon =
 * invisible tile, reader can't start). First-screen profile tile can't risk even that:
 * app-drawn AccountMark.qml.
 *
 * Bindings, not Row: positioner sizes in polish pass, never headless. Opens nothing; owning
 * page decides what answer means.
 */
Item {
    id: root

    /// Answers, tile order. Each an object:
    /// - `name`: handed back by `chosen`; tile is `<name>Tile`.
    /// - `icon`: theme icon name.
    /// - `mark`: app-drawn mark instead, for tiles that must show on every phone (`account`:
    ///   AccountMark.qml); `icon` then unread.
    /// - `text`: words. `hint`: optional quieter line under.
    /// - `enabled`: false greys out, ignores taps. Default on.
    property var choices: []

    /// Edge inset: margins on bare page, none inside inset column.
    property real sideMargin: Theme.horizontalPageMargin

    /// Stacked: three abreast leave no room for two-line words. Stacked tile is a row: icon
    /// left, words left (as ContactRow.qml).
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

    /// Tile height: tallest's, so row reads as one.
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
        // Unbuilt tile: no height yet; last built reports.
        onItemAdded: root.measure()
        onItemRemoved: root.measure()

        BackgroundItem {
            id: tile
            objectName: modelData.name + "Tile"
            x: root.sideMargin + (root.stacked ? 0 : index * root.tileWidth)
            y: root.stacked ? index * (root.tileHeight + root.gap) : 0
            width: root.tileWidth
            height: root.tileHeight
            // `undefined` = unmentioned: on.
            enabled: modelData.enabled === undefined || modelData.enabled

            /// Bottom of words.
            readonly property real wordsEnd: hint.visible
                ? hint.y + hint.height : caption.y + caption.height

            /// Content height: icon then words; stacked, whichever runs lower. Row draws all at
            /// largest.
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
                // Left as contact-row avatar; over words when abreast.
                x: root.stacked ? Theme.paddingMedium
                                : (tile.width - icon.width) / 2
                y: Theme.paddingMedium
                width: Theme.iconSizeMedium
                height: width
                // No theme ask where mark drawn.
                source: modelData.mark === undefined
                        ? "image://theme/" + modelData.icon + "?" + tile.tint
                        : ""
            }

            // Drawn mark in icon's place and colour.
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
                // Beside icon: top if two lines, else middle (as ContactRow.qml).
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
                // Under line above if beside icon; gap below if column.
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
