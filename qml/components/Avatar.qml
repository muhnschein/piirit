import QtQuick 2.0
import Sailfish.Silica 1.0

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
    color: avatar.showsPicture ? "transparent"
           : avatar.highlight ? Theme.highlightColor
           : avatar.monochrome ? Theme.rgba(Theme.primaryColor, 0.25)
           : ownColor.length > 0 ? ownColor : Theme.highlightColor

    Label {
        objectName: "avatarInitial"
        anchors.centerIn: parent
        visible: !avatar.showsPicture
        color: Theme.primaryColor
        font.pixelSize: Theme.fontSizeLarge
        textFormat: Text.PlainText
        text: avatar.initial.substring(0, 1).toUpperCase()
    }

    Image {
        id: picture
        objectName: "avatarImage"
        anchors.fill: parent
        // Never drawn itself: the face below draws its texture.
        visible: false
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        // Encoded per segment; see AttachmentPreview.qml's fileUrl.
        source: avatar.picturePath.length > 0
                ? Qt.resolvedUrl("file://" + avatar.picturePath.split("/")
                                                 .map(encodeURIComponent).join("/"))
                : ""
    }

    // The picture as it is seen: cut to a circle, grey where it is drawn
    // without its colour, and through the ambience's colour where the
    // cover lights it -- all of it arithmetic on each pixel of the
    // picture's own texture, in one pass straight to the screen.
    //
    // Nothing here is drawn into a texture of its own first, and that is
    // the point (issue #102). This used to be a Rectangle drawn as a mask,
    // an OpacityMask kept in a texture (`cached`), a Desaturate drawn out
    // of a layer, and a ColorOverlay kept in another: four or five
    // framebuffers per face, and every one of them made again whenever
    // the cover laid its grid out, which it does for every message. Qt
    // 5.6 never asks whether a framebuffer it made is any good. When the
    // phone cannot give it one, the texture it draws from is no texture,
    // which reads as opaque black -- a flat square, grey under the cover's
    // opacity, tinted under the overlay -- and it stays that way for as
    // long as the face does. A face with no framebuffer cannot lose one.
    //
    // The texture is the whole picture, not the part PreserveAspectCrop
    // would show, so the crop is done here too; and it may be a corner of
    // a texture atlas, which `qt_SubRect_source` says where, rather than
    // have Qt copy it out into a texture of its own.
    ShaderEffect {
        id: face
        objectName: "avatarFace"
        anchors.fill: parent
        visible: avatar.showsPicture

        property variant source: picture
        /// How much of the picture's width and height the circle takes:
        /// the middle of its long side, the whole of its short one.
        property size crop: picture.implicitWidth > picture.implicitHeight
                            ? Qt.size(picture.implicitHeight / picture.implicitWidth, 1)
                            : Qt.size(1, picture.implicitHeight > 0
                                         ? picture.implicitWidth / picture.implicitHeight
                                         : 1)
        /// Across, in pixels: the circle's rim is softened over one.
        property real diameter: Math.max(1, width)
        /// 1 takes the colour out. Taken out for the tint too: what goes
        /// through the highlight colour is a grey face, not a green one.
        property real desaturation: avatar.monochrome || avatar.highlight ? 1.0 : 0.0
        /// Kept to a part of the way so the face is still a face -- a
        /// full overlay is a silhouette, which says nothing about who
        /// wrote. Transparent is no tint at all.
        property color tint: avatar.highlight ? Theme.rgba(Theme.highlightColor, 0.75)
                                              : "transparent"

        fragmentShader: "
            varying highp vec2 qt_TexCoord0;
            uniform lowp float qt_Opacity;
            uniform lowp sampler2D source;
            uniform highp vec4 qt_SubRect_source;
            uniform highp vec2 crop;
            uniform highp float diameter;
            uniform lowp float desaturation;
            uniform highp vec4 tint;
            void main() {
                highp vec2 at = vec2(0.5) + (qt_TexCoord0 - vec2(0.5)) * crop;
                highp vec4 pixel = texture2D(source, qt_SubRect_source.xy + qt_SubRect_source.zw * at);
                highp float grey = (pixel.r + pixel.g + pixel.b) / 3.0;
                pixel.rgb = mix(pixel.rgb, vec3(grey), desaturation);
                pixel.rgb = mix(pixel.rgb / max(pixel.a, 0.00390625),
                                tint.rgb / max(tint.a, 0.00390625), tint.a) * pixel.a;
                highp float rim = clamp(diameter * 0.5 - length(qt_TexCoord0 - vec2(0.5)) * diameter + 0.5,
                                        0.0, 1.0);
                gl_FragColor = pixel * rim * qt_Opacity;
            }
        "
    }
}
