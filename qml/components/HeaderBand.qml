import QtQuick 2.0
import Sailfish.Silica 1.0

// Faint ambience colour behind a page's top block, hard edges: page's top to block's last
// line plus the header-to-picture gap, if any. Sibling before block: drawn under it.
Rectangle {
    /// Block's last line, in parent's coordinates.
    property real lastBottom
    /// Header's bottom, picture's top: their gap repeats under last line. Unset: none.
    property real headerBottom
    property real pictureTop

    objectName: "headerBand"
    width: parent ? parent.width : 0
    height: lastBottom + (pictureTop - headerBottom)
    // Strength as drawn on the canvas, any ambience.
    color: Theme.rgba(Theme.highlightBackgroundColor, 0.12)
}
