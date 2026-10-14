import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Intro picture (pages/IntroPage.qml), painted ahead by tools/faces/scenes.py into
 * qml/art/.
 *
 * Red = theme primary, green = highlight: one file fits every ambience.
 *
 * Square, not letterboxed.
 */
Item {
    id: art

    /// Picture URL, relative to the file that sets it.
    property url source

    /// Drawing colour; accents in litColour.
    property color colour: Theme.primaryColor
    property color litColour: Theme.highlightColor
    /// Strength; near full for a picture.
    property real ink: 0.9
    property real litInk: 1.0

    /// Image loaded.
    readonly property bool ready: drawing.status === Image.Ready

    // Off main thread, drawn size, ambience colours (src/pictures.rs):
    // red = `colour`, green = `litColour`. Scaled from master first.
    //
    // Not a shader: one made after window hid got another's program (Avatar.qml).
    Image {
        id: drawing
        anchors.fill: parent
        visible: art.ready
        asynchronous: true
        sourceSize.width: Math.round(art.width)
        sourceSize.height: Math.round(art.height)
        source: ("" + art.source).length > 0 && art.width > 0
                ? "image://piirit/ink?file=" + encodeURIComponent("" + art.source)
                  + "&ink=" + encodeURIComponent("" + art.colour)
                  + "&lit=" + encodeURIComponent("" + art.litColour)
                  + "&inkStrength=" + art.ink + "&litStrength=" + art.litInk
                : ""
    }
}
