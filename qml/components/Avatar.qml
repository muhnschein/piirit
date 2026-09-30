import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * A round avatar: the subject's own colour with their initial on it, or
 * their picture drawn through a circular mask.
 *
 * The mask is the whole point. An Image does not inherit its parent's
 * corner radius, and `clip` cuts only to the bounding box, so a picture
 * left to itself reads as a square among circles.
 *
 * The picture is masked, greyed and tinted in ONE pass over the
 * picture's own texture. It used to be three effects in a row -- an
 * `OpacityMask` keeping its result in a texture, that result greyed
 * through a `layer.effect`, and a `ColorOverlay` keeping *its* result in
 * a texture -- and every one of those textures is a drawing of the
 * avatar rather than the avatar's own. A texture like that can come back
 * blank and is then drawn blank for good: on the cover, where every cell
 * is grey or tinted and so every cell went through all three, whole
 * grids of avatars turned into flat squares after a long run (issue
 * #102). Here there is nothing between the picture and the screen: the
 * circle, the desaturation and the tint are arithmetic on each pixel of
 * the picture's own texture, redone whenever it is drawn. The circle
 * cannot come out square, because its alpha is computed here rather than
 * kept somewhere.
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
        visible: false
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        // Encoded per segment; see AttachmentPreview.qml's fileUrl.
        source: avatar.picturePath.length > 0
                ? Qt.resolvedUrl("file://" + avatar.picturePath.split("/")
                                         .map(encodeURIComponent).join("/"))
                : ""
    }

    // The picture, masked and dressed. `source` is the picture's own
    // texture and this draws it again on every frame it is shown: no
    // drawing of the avatar is kept between frames anywhere.
    ShaderEffect {
        id: painted
        objectName: "avatarMasked"
        anchors.fill: parent
        visible: avatar.showsPicture
        property variant source: picture
        /// The circle softens over about a pixel at its rim, the way the
        /// rounded rectangle the mask used to be was antialiased.
        property real rim: 2.0 / Math.max(1, avatar.width)
        /// The colour taken out: 1 for the cover, which draws nobody in
        /// their own colours, and 0 for a chat list.
        property real grey: avatar.monochrome || avatar.highlight ? 1.0 : 0.0
        /// The ambience's colour, kept to a part of the way so the face
        /// is still a face -- a full overlay is a silhouette, which says
        /// nothing about who wrote. Transparent where nothing is laid
        /// over the picture at all.
        property color tint: avatar.highlight ? Theme.rgba(Theme.highlightColor, 0.75)
                                              : Qt.rgba(0, 0, 0, 0)

        // The colour is premultiplied, so the whole of it goes with the
        // mask's alpha. Pieced together line by line rather than written
        // as one string over several lines, which QML takes and a script
        // need not -- the way qml/js/QuickActions.js keeps its shader.
        fragmentShader:
            "varying highp vec2 qt_TexCoord0;\n" +
            "uniform sampler2D source;\n" +
            "uniform highp float rim;\n" +
            "uniform lowp float grey;\n" +
            "uniform lowp vec4 tint;\n" +
            "uniform lowp float qt_Opacity;\n" +
            "void main() {\n" +
            "    lowp vec4 face = texture2D(source, qt_TexCoord0);\n" +
            "    highp vec2 fromMiddle = qt_TexCoord0 - vec2(0.5, 0.5);\n" +
            "    highp float across = length(fromMiddle) * 2.0;\n" +
            "    lowp float circle = 1.0 - smoothstep(1.0 - rim, 1.0, across);\n" +
            "    lowp float luma = dot(face.rgb, vec3(0.2126, 0.7152, 0.0722));\n" +
            "    lowp vec3 body = mix(face.rgb, vec3(luma), grey);\n" +
            "    body = mix(body, tint.rgb, tint.a);\n" +
            "    lowp float alpha = face.a * circle;\n" +
            "    gl_FragColor = vec4(body * alpha, alpha) * qt_Opacity;\n" +
            "}\n"
    }
}