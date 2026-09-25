import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Media.js" as Media

/*
 * What a chat holds besides words, as a row of tiles on the page that
 * says who the chat is with: pictures and videos, sounds, files, and --
 * where apps are on -- apps; on a contact's page, where calls are on,
 * the calls with them too. Each tile is the way into the page that
 * lists that kind (pages/ChatMediaPage.qml), which is where
 * deltachat-android and deltachat-ios keep theirs as well.
 *
 * A row of tiles rather than a list of rows, because these are doors and
 * not settings: four icons with a word under each read at a glance, and
 * the page under them stays short. Each tile is Silica's BackgroundItem,
 * so it lights up under a thumb the way every other tappable thing on
 * the phone does, and its icon is the theme's own for that kind of file
 * -- the ones the platform's file browser draws -- in the theme's own
 * colours. Apps are the exception: the theme has no icon for those, and
 * AppMark says why it draws one.
 *
 * Laid out by bindings rather than a Row, for the reason MessageDelegate
 * is: a positioner sizes itself in a polish pass, which never runs
 * headlessly.
 *
 * The row keeps its own room above and below the tiles. They are a third
 * kind of thing between who the chat is with and what the chat does, and
 * a column's own spacing does not say so; the page that puts them in a
 * column gets the gap without having to know how wide one should be.
 *
 * Nothing is opened here. The page that owns the pageStack pushes the
 * media page, which keeps this loadable, and testable, on its own.
 */
Item {
    id: root

    /// Whether webxdc apps are on. Off, there is no Apps tile, the way
    /// the attach tray has no app entry then.
    property bool appsAvailable: false

    /// Whether there is a Calls tile: calls are on, and the chat is one a
    /// call can be placed in. A contact's page sets it; a group's never.
    property bool callsAvailable: false

    /// The reader tapped a kind: gallery, audio, files, apps or calls.
    signal kindRequested(string kind)

    /// The kinds on offer, in tile order.
    readonly property var kinds: Media.kinds(root.appsAvailable, root.callsAvailable)
    /// How wide one tile is: the row shared out between them, inside the
    /// page margins.
    readonly property real tileWidth: root.kinds.length > 0
        ? (root.width - 2 * Theme.horizontalPageMargin) / root.kinds.length
        : 0

    /// The room kept clear above and below the tiles, over and above the
    /// padding a tile carries inside its own highlight. A little more
    /// above than below: the combo under the tiles keeps a line's room of
    /// its own above its label, and the caption over them keeps none, so
    /// the two gaps come out about even on screen.
    readonly property real gapAbove: Theme.paddingLarge + Theme.paddingMedium
    readonly property real gapBelow: Theme.paddingLarge
    /// How tall one tile is: an icon, a gap, one line of caption, and the
    /// padding a row keeps above and below.
    readonly property real tileHeight: Theme.iconSizeMedium + Theme.paddingSmall
                                       + captionMetric.height + 2 * Theme.paddingMedium

    width: parent ? parent.width : 0
    height: root.gapAbove + root.tileHeight + root.gapBelow

    // One line of the caption's font, measured once: every tile's word
    // is one line of it.
    Label {
        id: captionMetric
        visible: false
        font.pixelSize: Theme.fontSizeSmall
        text: "Ag"
    }

    Repeater {
        model: root.kinds

        BackgroundItem {
            id: tile
            objectName: modelData + "Tile"
            x: Theme.horizontalPageMargin + index * root.tileWidth
            y: root.gapAbove
            width: root.tileWidth
            height: root.tileHeight

            readonly property bool isApps: modelData === "apps"
            readonly property color tint: tile.highlighted ? Theme.highlightColor
                                                            : Theme.primaryColor

            // The theme's icon in the theme's colour, lit while the tile
            // is pressed, as an IconButton's would be.
            Image {
                id: icon
                objectName: "tileIcon"
                visible: !tile.isApps
                anchors {
                    top: parent.top
                    topMargin: Theme.paddingMedium
                    horizontalCenter: parent.horizontalCenter
                }
                width: Theme.iconSizeMedium
                height: width
                source: tile.isApps ? ""
                        : "image://theme/" + Media.kindIcon(modelData) + "?" + tile.tint
            }

            // Drawn rather than named; see AppMark. A little smaller than
            // the icon's box, which is how much of that box an icon fills.
            AppMark {
                objectName: "tileMark"
                visible: tile.isApps
                anchors.centerIn: icon
                size: Theme.iconSizeMedium * 0.6
                color: tile.tint
            }

            Label {
                objectName: "tileLabel"
                anchors {
                    top: icon.bottom
                    topMargin: Theme.paddingSmall
                    left: parent.left
                    right: parent.right
                }
                horizontalAlignment: Text.AlignHCenter
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeSmall
                color: tile.tint
                text: Media.kindName(modelData)
            }

            onClicked: root.kindRequested(modelData)
        }
    }
}
