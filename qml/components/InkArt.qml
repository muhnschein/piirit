import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * A picture in the ambience's colours: one of the drawings the
 * introduction (pages/IntroPage.qml) puts above each fact, painted ahead
 * of time by tools/faces/scenes.py into qml/art/.
 *
 * Two channels rather than a colour picture: what ships has no colour in
 * it, the red channel is what the theme's primary colour draws and the
 * green what its highlight draws, so one file is right on every ambience
 * -- a light one included -- and nothing has to be redrawn when Sailfish
 * gains another.
 *
 * The pictures are square and this does not letterbox: give it a square.
 */
Item {
    id: art

    /// The picture, as a URL relative to the file that sets it.
    property url source

    /// The colours: the drawing in the first, its accents in the second.
    property color colour: Theme.primaryColor
    property color litColour: Theme.highlightColor
    /// How much of each. A picture is what the reader is looking at, so
    /// it is drawn nearly full strength -- unlike the field behind it.
    property real ink: 0.9
    property real litInk: 1.0

    /// True once there is something to draw.
    readonly property bool ready: drawing.status === Image.Ready

    // Painted off the main thread at the size it is drawn, in the
    // ambience's colours (src/pictures.rs): red is how much of `colour`,
    // green how much of `litColour`. Scaled down from the master before
    // it is painted, smoothly, which is what mipmaps did for the shader
    // this was.
    //
    // Not a shader any more: one made after the window had once been
    // hidden -- the introduction can come up after the app was in the
    // background -- could be drawn on the phone's Qt with another one's
    // program (Avatar.qml says how).
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
