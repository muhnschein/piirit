import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Media.js" as Media

/*
 * Chat media tiles: gallery, sounds, files, apps (if on), calls (contact chats), each to
 * pages/ChatMediaPage.qml, as deltachat-android/-ios. Tiles: doors, not settings. AppMark
 * draws apps' missing theme icon. Bindings, not Row: polish never runs headless. Own room
 * above/below for page columns. Opens nothing (page pushes), so testable alone.
 */
Item {
    id: root

    /// Webxdc apps on. Off: no Apps tile.
    property bool appsAvailable: false

    /// Calls tile: contact chats only.
    property bool callsAvailable: false

    /// Tapped: gallery, audio, files, apps or calls.
    signal kindRequested(string kind)

    /// Kinds on offer, tile order.
    readonly property var kinds: Media.kinds(root.appsAvailable, root.callsAvailable)
    /// Tile width: row split inside page margins.
    readonly property real tileWidth: root.kinds.length > 0
        ? (root.width - 2 * Theme.horizontalPageMargin) / root.kinds.length
        : 0

    /// Room around tiles; more above (combo below has line room, caption none).
    readonly property real gapAbove: Theme.paddingLarge + Theme.paddingMedium
    readonly property real gapBelow: Theme.paddingLarge
    /// Tile height: icon, gap, one caption line, row padding.
    readonly property real tileHeight: Theme.iconSizeMedium + Theme.paddingSmall
                                       + captionMetric.height + 2 * Theme.paddingMedium

    width: parent ? parent.width : 0
    height: root.gapAbove + root.tileHeight + root.gapBelow

    // One caption line, measured once.
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

            // Theme icon, lit while pressed (as IconButton).
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

            // Drawn, not named (see AppMark). 0.6 = icon's fill of its box.
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
