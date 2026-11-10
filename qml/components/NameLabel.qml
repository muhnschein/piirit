import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Emoji.js" as Emoji

/*
 * Someone's chosen name, as written: emoji as Twemoji (Emoji.js), as chat list draws
 * names; else plain text.
 */
Label {
    id: root

    property string name

    readonly property string pictured: Emoji.inText(root.name, false, root.font.pixelSize, "",
                                                    Emoji.nameSaturation)

    textFormat: root.pictured.length > 0 ? Text.StyledText : Text.PlainText
    text: root.pictured.length > 0 ? root.pictured : root.name
}
