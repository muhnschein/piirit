import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/QuickActions.js" as QuickActions

/*
 * Round avatar: colour and initial, or picture cut to circle.
 *
 * Circle is the point: Image ignores parent corner radius and `clip` cuts to
 * the bounding box, so a bare picture reads as a square.
 *
 * Shared by chat and contact lists.
 */
Rectangle {
    id: avatar

    /// Fallback when no picture.
    property string initial
    /// Core's colour.
    property string ownColor
    /// Picture path; empty: none.
    property string picturePath
    /// Grey, desaturated: cover grid.
    property bool monochrome: false
    /// Ambience colour, not subject's: `Theme.highlightColor` disc, picture tinted
    /// through it. Cover marks who wrote with it (unread badge colour).
    property bool highlight: false
    /// Fade top-down, fractions of height: full above `fadeFrom`, gone by `fadeTo`.
    /// Equal, or starting below avatar's foot: no fade.
    property real fadeFrom: 0
    property real fadeTo: 0
    readonly property bool fades: avatar.fadeTo > avatar.fadeFrom && avatar.fadeFrom < 1

    /// Picture shown: exists and loaded. Picture and stand-in key off this.
    ///
    /// Loads off main thread: row shows before picture. Path alone left disc blank.
    /// Unloadable picture falls back to stand-in.
    readonly property bool showsPicture: avatar.picturePath.length > 0
                                         && picture.status === Image.Ready

    width: Theme.itemSizeSmall
    height: width
    radius: width / 2
    // Disc colour only when seen: behind picture it tinted the mask edge, and
    // showed full disc when picture skipped a frame.
    readonly property color discColor: avatar.showsPicture ? "transparent"
           : avatar.highlight ? Theme.highlightColor
           : avatar.monochrome ? Theme.rgba(Theme.primaryColor, 0.25)
           : ownColor.length > 0 ? ownColor : Theme.highlightColor
    color: avatar.discColor
    // Fading disc: steps fine enough to read as a curve.
    gradient: avatar.fades && !avatar.showsPicture ? avatar.fade : null

    /// Disc colour `at` down the fade.
    function faded(at) {
        var c = avatar.discColor
        return Qt.rgba(c.r, c.g, c.b, c.a * QuickActions.fade(at, avatar.fadeFrom, avatar.fadeTo))
    }

    /// Fade gradient, made on first fade (cover only); null until then, else every
    /// avatar in every list built 8 stops and bindings.
    property QtObject fade: null
    function makeFade() {
        if (avatar.fades && avatar.fade === null) {
            avatar.fade = fadeMaker.createObject(avatar)
        }
    }
    onFadesChanged: avatar.makeFade()
    Component.onCompleted: avatar.makeFade()

    Component {
        id: fadeMaker
        Gradient {
            GradientStop { position: 0.0; color: avatar.faded(0.0) }
            GradientStop { position: 0.2; color: avatar.faded(0.2) }
            GradientStop { position: 0.4; color: avatar.faded(0.4) }
            GradientStop { position: 0.6; color: avatar.faded(0.6) }
            GradientStop { position: 0.7; color: avatar.faded(0.7) }
            GradientStop { position: 0.8; color: avatar.faded(0.8) }
            GradientStop { position: 0.9; color: avatar.faded(0.9) }
            GradientStop { position: 1.0; color: avatar.faded(1.0) }
        }
    }

    Label {
        objectName: "avatarInitial"
        anchors.centerIn: parent
        visible: !avatar.showsPicture
        // Letter fades in place: too small to fade across.
        opacity: avatar.fades ? QuickActions.fade(0.5, avatar.fadeFrom, avatar.fadeTo) : 1
        color: Theme.primaryColor
        font.pixelSize: Theme.fontSizeLarge
        textFormat: Text.PlainText
        text: avatar.initial.substring(0, 1).toUpperCase()
    }

    Image {
        id: picture
        objectName: "avatarImage"
        anchors.fill: parent
        visible: avatar.showsPicture
        asynchronous: true
        // Made at drawn size: circle rim one pixel.
        sourceSize.width: Math.round(avatar.width)
        sourceSize.height: Math.round(avatar.height)
        // Picture prepared in src/pictures.rs: circle cut, grey (monochrome),
        // tinted (highlight), faded. Not a shader: on Sailfish Qt 5.6 a
        // ShaderEffect made after window was hidden can draw with another's
        // program (cover fade turned faces into white squares). Image uses Qt's
        // texture material.
        source: avatar.picturePath.length > 0 && avatar.width > 0
                ? "image://piirit/face?file=" + encodeURIComponent(avatar.picturePath)
                  + (avatar.monochrome ? "&grey=1" : "")
                  + (avatar.highlight
                     ? "&tint=" + encodeURIComponent("" + Theme.highlightColor) + "&strength=0.75"
                     : "")
                  + (avatar.fades ? "&from=" + avatar.fadeFrom + "&to=" + avatar.fadeTo : "")
                : ""
    }
}
