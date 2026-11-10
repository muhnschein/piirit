import QtQuick 2.0
import Sailfish.Silica 1.0

// Faint ambience colour behind a page's top block, hard edges: header's bottom to block's
// last line plus the header-to-picture gap. Sibling before page's Column: drawn under it.
Rectangle {
    /// Header's bottom, picture's top, block's last line: in Column's coordinates.
    property real headerBottom
    property real pictureTop
    property real lastBottom

    objectName: "headerBand"
    width: parent ? parent.width : 0
    y: headerBottom
    height: lastBottom + (pictureTop - headerBottom) - headerBottom
    // Strength as drawn on the canvas, any ambience.
    color: Theme.rgba(Theme.highlightBackgroundColor, 0.12)
}
