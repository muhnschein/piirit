import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * One thing the cover says about a call it shows: the microphone is off,
 * the camera is paused. A pill with an icon and a few words, on a ground
 * of its own -- dark, in a dark ambience -- so it reads over the other
 * end's face, in letters larger than a cover's usual: it is said to
 * someone glancing at the home screen, from arm's length.
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
    /// The most it may take across; it shrinks its words to fit.
    property real maximumWidth: Screen.width

    readonly property color ink: note.loud ? Theme.highlightColor : Theme.primaryColor

    width: Math.min(note.maximumWidth,
                    2 * Theme.paddingLarge + icon.width + Theme.paddingSmall
                    + words.implicitWidth)
    height: Math.max(icon.height, words.implicitHeight) + 2 * Theme.paddingSmall
    radius: height / 2
    color: Qt.rgba(note.ground.r, note.ground.g, note.ground.b, 0.45)
    border.width: note.loud ? Math.max(1, Math.round(Theme.paddingSmall / 4)) : 0
    border.color: Theme.highlightColor

    Image {
        id: icon
        anchors {
            left: parent.left
            leftMargin: Theme.paddingLarge
            verticalCenter: parent.verticalCenter
        }
        width: Theme.iconSizeSmall
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
            leftMargin: Theme.paddingSmall
            right: parent.right
            rightMargin: Theme.paddingLarge
            verticalCenter: parent.verticalCenter
        }
        color: note.ink
        font.pixelSize: Theme.fontSizeMedium
        // Shrunk to fit rather than cut, as the count of new messages is:
        // a long word for "paused" still has to be read whole.
        fontSizeMode: Text.HorizontalFit
        minimumPixelSize: Theme.fontSizeTiny
        textFormat: Text.PlainText
        text: note.text
    }
}
