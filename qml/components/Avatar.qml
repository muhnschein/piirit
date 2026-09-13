import QtQuick 2.0
import QtGraphicalEffects 1.0
import Sailfish.Silica 1.0

/*
 * A round avatar: the subject's own colour with their initial on it, or
 * their picture drawn through a circular mask.
 *
 * The mask is the whole point. An Image does not inherit its parent's
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
        visible: false
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        // Encoded per segment; see AttachmentPreview.qml's fileUrl.
        source: avatar.picturePath.length > 0
                ? Qt.resolvedUrl("file://" + avatar.picturePath.split("/")
                                                 .map(encodeURIComponent).join("/"))
                : ""
    }

    Rectangle {
        id: mask
        objectName: "avatarMask"
        anchors.fill: parent
        radius: width / 2
        visible: false
    }

    OpacityMask {
        id: masked
        objectName: "avatarMasked"
        anchors.fill: parent
        // Hidden while it is what the tint below is drawn from: an
        // effect draws its source itself, and both on screen would be
        // the same face twice.
        visible: avatar.showsPicture && !avatar.highlight
        source: picture
        maskSource: mask
        // An effect re-runs its shader whenever what it draws is redrawn,
        // and a list redraws its rows on every frame of a scroll. An
        // avatar does not change between those frames, so the result is
        // kept in a texture and reused until the picture does change.
        cached: true
        // The colour taken out on the way to the screen, as a layer over
        // the masked picture, so the mask and the desaturation are one
        // texture rather than two effects drawn over each other. Taken
        // out for the tint too: what goes through the highlight colour
        // is a grey face, not a green one.
        layer.enabled: avatar.monochrome || avatar.highlight
        layer.effect: Desaturate {
            desaturation: 1.0
        }
    }

    // The picture in the ambience's colour: the grey masked face with
    // the highlight over it, rather than the face's own colours. Kept
    // to a part of the way so the face is still a face -- a full
    // overlay is a silhouette, which says nothing about who wrote.
    ColorOverlay {
        objectName: "avatarTinted"
        anchors.fill: parent
        visible: avatar.showsPicture && avatar.highlight
        source: masked
        color: Theme.rgba(Theme.highlightColor, 0.75)
        cached: true
    }
}
