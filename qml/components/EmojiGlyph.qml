import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Emoji.js" as Emoji

/*
 * One emoji as picture: Qt 5.6 can't draw colour fonts, system emoji mono, few.
 * Twemoji where shipped (qml/art/emoji/, scripts/fetch-emoji.py), else text
 * (core doesn't verify emoji).
 */
Item {
    id: root

    /// Emoji, as sent or picked.
    property string emoji
    /// Drawn edge length.
    property real size: Theme.fontSizeMedium

    /// Picture name; "" text.
    readonly property string picture: Emoji.pictureFor(root.emoji)
    /// Picture showing.
    readonly property bool drawn: root.picture.length > 0 && image.status !== Image.Error

    implicitWidth: root.drawn ? root.size : fallback.implicitWidth
    implicitHeight: root.size
    width: implicitWidth
    height: implicitHeight

    Image {
        id: image
        objectName: "emojiImage"
        anchors.centerIn: parent
        width: root.size
        height: root.size
        visible: root.drawn
        // 72px, decoded at drawn size off UI thread: pickers hold hundreds.
        asynchronous: true
        sourceSize.width: Math.ceil(root.size)
        sourceSize.height: Math.ceil(root.size)
        smooth: true
        fillMode: Image.PreserveAspectFit
        source: root.picture.length > 0
                ? Qt.resolvedUrl("../art/emoji/" + root.picture + ".png") : ""
    }

    Label {
        id: fallback
        objectName: "emojiText"
        anchors.centerIn: parent
        visible: !root.drawn
        // Em taller than glyph.
        font.pixelSize: Math.round(root.size * 0.85)
        // Other end's text: see MessageDelegate.
        textFormat: Text.PlainText
        // No layout behind picture.
        text: root.drawn ? "" : root.emoji
    }
}
