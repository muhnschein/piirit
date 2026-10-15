import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Intro picture (pages/IntroPage.qml) from tools/faces/scenes.py into qml/art/.
 * Square. Red/green recoloured per ambience: one file fits all.
 */
Item {
    id: art

    /// Picture URL, relative to setting file.
    property url source

    /// Drawing colour; accents in litColour.
    property color colour: Theme.primaryColor
    property color litColour: Theme.highlightColor
    /// Strength; near full.
    property real ink: 0.9
    property real litInk: 1.0

    /// Image loaded.
    readonly property bool ready: drawing.status === Image.Ready

    // Off main thread at drawn size (src/pictures.rs): red = `colour`, green =
    // `litColour`. No shader: one made after window hid got another's program
    // (Avatar.qml).
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
