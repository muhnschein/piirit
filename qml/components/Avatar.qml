import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/QuickActions.js" as QuickActions

/*
 * Round avatar: colour and initial, or picture cut to circle (Image ignores parent
 * radius, `clip` cuts to box). Chat and contact lists.
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
    /// Ambience `Theme.highlightColor` disc, not own; picture tinted with it. Cover marks who wrote
    /// (badge colour).
    property bool highlight: false
    /// Top-down fade, height fractions: full above `fadeFrom`, gone by `fadeTo`. Equal, or
    /// below foot: none.
    property real fadeFrom: 0
    property real fadeTo: 0
    readonly property bool fades: avatar.fadeTo > avatar.fadeFrom && avatar.fadeFrom < 1

    /// Picture exists and loaded; picture and stand-in key off it. Async load (row before picture):
    /// path alone left disc blank. Unloadable: stand-in.
    readonly property bool showsPicture: avatar.picturePath.length > 0
                                         && picture.status === Image.Ready

    width: Theme.itemSizeSmall
    height: width
    radius: width / 2
    // Disc colour only when seen: behind picture it tinted mask edge, flashed on skipped frame.
    readonly property color discColor: avatar.showsPicture ? "transparent"
           : avatar.highlight ? Theme.highlightColor
           : avatar.monochrome ? Theme.rgba(Theme.primaryColor, 0.25)
           : ownColor.length > 0 ? ownColor : Theme.highlightColor
    color: avatar.discColor
    // Fade steps fine enough for a curve.
    gradient: avatar.fades && !avatar.showsPicture ? avatar.fade : null

    /// Disc colour `at` down the fade.
    function faded(at) {
        var c = avatar.discColor
        return Qt.rgba(c.r, c.g, c.b, c.a * QuickActions.fade(at, avatar.fadeFrom, avatar.fadeTo))
    }

    /// Fade gradient, made on first fade (cover only), else every avatar built 8 stops.
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
        // Letter fades whole: too small to fade across.
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
        // Drawn size: one-pixel rim.
        sourceSize.width: Math.round(avatar.width)
        sourceSize.height: Math.round(avatar.height)
        // src/pictures.rs cuts, greys, tints, fades. No shader: Sailfish Qt 5.6
        // ShaderEffect made after window hid can use another's program (cover faces
        // went white squares).
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
