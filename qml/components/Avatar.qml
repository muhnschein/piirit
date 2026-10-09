import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/QuickActions.js" as QuickActions

/*
 * A round avatar: the subject's own colour with their initial on it, or
 * their picture cut to a circle.
 *
 * The circle is the whole point. An Image does not inherit its parent's
 * corner radius, and `clip` cuts only to the bounding box, so a picture
 * left to itself reads as a square among circles.
 *
 * Shared by the chat list and the contact lists so the two cannot drift.
 */
Rectangle {
    id: avatar

    /// Shown when there is no picture.
    property string initial
    /// The core's per-chat or per-contact colour.
    property string ownColor
    /// Path to the picture, empty when there is none.
    property string picturePath
    /// Drawn without its colour: grey behind the initial, the picture
    /// desaturated. For the cover's grid of everyone, where nobody is
    /// drawn in their own colours.
    property bool monochrome: false
    /// Drawn in the ambience's own colour instead of the subject's:
    /// `Theme.highlightColor` behind the initial, the picture through
    /// the same colour. What the cover does for whoever has written, so
    /// a face with something new reads as the phone's own highlight --
    /// the colour the unread badge in the chat list is already drawn in
    /// -- rather than as one more photograph among grey ones.
    property bool highlight: false
    /// Where the avatar fades out, as fractions of its height from its
    /// top: whole above `fadeFrom`, gone by `fadeTo`. The cover's grid
    /// sinks away into the strip its quick actions are drawn in. Equal,
    /// as they start, is no fade, and so is one that starts below the
    /// avatar's foot.
    property real fadeFrom: 0
    property real fadeTo: 0
    readonly property bool fades: avatar.fadeTo > avatar.fadeFrom && avatar.fadeFrom < 1

    /// The picture is what is on screen: there is one, and it has
    /// loaded. Everything that draws a picture and everything that
    /// stands in for one turns on this one fact, so the two never both
    /// stand down.
    ///
    /// A picture is loaded off the main thread, which is what keeps a
    /// list smooth while it is scrolled -- and what it costs is that a
    /// row is on screen before its picture is. Reading "there is a path"
    /// as "there is a picture" left the disc transparent and the initial
    /// hidden for that time, so a chat list arrived as a column of holes
    /// that filled in one by one. It arrives as a column of faces now:
    /// the subject's colour and their initial until their picture is
    /// ready, and their picture from then on. It is also what a picture
    /// that never loads -- a blob deleted under the app, a file that is
    /// not an image -- falls back to, instead of a hole for good.
    readonly property bool showsPicture: avatar.picturePath.length > 0
                                         && picture.status === Image.Ready

    width: Theme.itemSizeSmall
    height: width
    radius: width / 2
    // The disc's own colour only when it is what is seen: behind a
    // picture it showed as a tinted ring where the mask's edge is
    // softened, and as a full tinted disc whenever the masked picture
    // was not drawn for a frame -- a row highlighted under its context
    // menu was where that was noticed.
    readonly property color discColor: avatar.showsPicture ? "transparent"
           : avatar.highlight ? Theme.highlightColor
           : avatar.monochrome ? Theme.rgba(Theme.primaryColor, 0.25)
           : ownColor.length > 0 ? ownColor : Theme.highlightColor
    color: avatar.discColor
    // Fading, the disc goes the way the picture does, in steps close
    // enough that the eye takes them for the curve.
    gradient: avatar.fades && !avatar.showsPicture ? fade : null

    /// The disc's colour where it is `at` of the way down.
    function faded(at) {
        var c = avatar.discColor
        return Qt.rgba(c.r, c.g, c.b, c.a * QuickActions.fade(at, avatar.fadeFrom, avatar.fadeTo))
    }

    Gradient {
        id: fade
        GradientStop { position: 0.0; color: avatar.faded(0.0) }
        GradientStop { position: 0.2; color: avatar.faded(0.2) }
        GradientStop { position: 0.4; color: avatar.faded(0.4) }
        GradientStop { position: 0.6; color: avatar.faded(0.6) }
        GradientStop { position: 0.7; color: avatar.faded(0.7) }
        GradientStop { position: 0.8; color: avatar.faded(0.8) }
        GradientStop { position: 0.9; color: avatar.faded(0.9) }
        GradientStop { position: 1.0; color: avatar.faded(1.0) }
    }

    Label {
        objectName: "avatarInitial"
        anchors.centerIn: parent
        visible: !avatar.showsPicture
        // A letter is too small to fade across; it takes the fade where
        // it stands.
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
        // Made at the size it is drawn, so the circle's rim is one pixel
        // wide on screen as it was in the shader.
        sourceSize.width: Math.round(avatar.width)
        sourceSize.height: Math.round(avatar.height)
        // The picture as it is seen, made ready before it reaches the
        // screen (src/pictures.rs): cut to a circle, grey where it is drawn
        // without its colour, through the ambience's colour where the
        // cover lights it, and fading where the cover's grid does.
        //
        // Not drawn by a shader, and that is the point. A ShaderEffect
        // made after its window has once been hidden can be drawn on
        // Sailfish's Qt 5.6 with another ShaderEffect's program -- the
        // cover's own fade, which turned every new face into a white
        // square of its whole picture -- and the cover makes faces for
        // every message. An Image is drawn with Qt's own texture
        // material, which nothing can mistake for another.
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
