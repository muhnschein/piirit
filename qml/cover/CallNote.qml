import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * One thing the cover says about a call it shows: the microphone is off,
 * the camera is paused. A pill with an icon and a few words, on a ground
 * of its own -- dark, in a dark ambience -- so it reads over the other
 * end's face. Small, as a cover's words are, and smaller than the name
 * and the time above it: two of them have to fit under those, and over
 * the action, on a cover a third of the screen wide.
 *
 * What the reader can do something about is drawn loud -- in the
 * ambience's highlight, with a thin outline in it, the way the cover's
 * count of new messages is -- and what is only so is drawn in the
 * primary colour.
 */
Rectangle {
    id: note

    /// What it says.
    property string text
    /// The theme's icon beside it, by name.
    property string iconName
    /// Something the reader can act on, rather than only a fact.
    property bool loud: false
    /// What it is drawn on: black in a dark ambience, white in a light
    /// one, so the ambience's own colours read on it.
    property color ground: Qt.rgba(0, 0, 0, 1)
    /// The most it may take across. Its words are shrunk a little to fit,
    /// and cut short past that.
    property real maximumWidth: Screen.width
    /// Its sizes, as a share of the theme's: less than one on a cover
    /// smaller than the large one.
    property real ratio: 1

    readonly property color ink: note.loud ? Theme.highlightColor : Theme.primaryColor
    /// The size of its words, and of its icon, which stands as tall as
    /// they are.
    readonly property real size: Math.round(Theme.fontSizeExtraSmall * note.ratio)
    /// The room around its icon and words.
    readonly property real across: Math.round(Theme.paddingMedium * note.ratio)
    readonly property real between: Math.round(Theme.paddingSmall * note.ratio)

    width: Math.min(note.maximumWidth,
                    2 * note.across + icon.width + note.between + words.implicitWidth)
    height: icon.height + 2 * note.between
    radius: height / 2
    color: Qt.rgba(note.ground.r, note.ground.g, note.ground.b, 0.45)
    border.width: note.loud ? Math.max(1, Math.round(Theme.paddingSmall / 4)) : 0
    border.color: Theme.highlightColor

    Image {
        id: icon
        objectName: "callNoteIcon"
        anchors {
            left: parent.left
            leftMargin: note.across
            verticalCenter: parent.verticalCenter
        }
        width: Math.round(note.size)
        height: width
        sourceSize.width: width
        sourceSize.height: height
        source: note.iconName.length > 0
                ? "image://theme/" + note.iconName + "?" + note.ink
                : ""
    }

    Label {
        id: words
        objectName: "callNoteText"
        anchors {
            left: icon.right
            leftMargin: note.between
            right: parent.right
            rightMargin: note.across
            verticalCenter: parent.verticalCenter
        }
        color: note.ink
        font.pixelSize: note.size
        // Shrunk a little to fit, as the count of new messages is, and cut
        // short past that rather than run out of the pill.
        fontSizeMode: Text.HorizontalFit
        minimumPixelSize: Math.round(Theme.fontSizeTiny * note.ratio)
        truncationMode: TruncationMode.Elide
        textFormat: Text.PlainText
        text: note.text
    }
}
