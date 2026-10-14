import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Cover note about a call (mic off, camera paused): icon and words on own
 * ground. Actionable items loud (highlight, thin outline); plain facts primary
 * colour.
 */
Rectangle {
    id: note

    /// Text.
    property string text
    /// Theme icon name.
    property string iconName
    /// Actionable, not a fact.
    property bool loud: false
    /// Ground: black in dark ambience, else white.
    property color ground: Qt.rgba(0, 0, 0, 1)
    /// Max width; words shrink, then cut.
    property real maximumWidth: Screen.width
    /// Sizes as theme share: < 1 on covers smaller than the large one.
    property real ratio: 1

    readonly property color ink: note.loud ? Theme.highlightColor : Theme.primaryColor
    /// Word size; icon as tall.
    readonly property real size: Math.round(Theme.fontSizeExtraSmall * note.ratio)
    /// Room around icon and words.
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
        // Shrink to fit, then cut; never overrun pill.
        fontSizeMode: Text.HorizontalFit
        minimumPixelSize: Math.round(Theme.fontSizeTiny * note.ratio)
        truncationMode: TruncationMode.Elide
        textFormat: Text.PlainText
        text: note.text
    }
}
