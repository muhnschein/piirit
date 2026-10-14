import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Format.js" as Format
import "."

/*
 * Search result, any kind. Laid out like chat-list row. Names and message text
 * from others: PlainText (MessageDelegate.qml).
 */
Item {
    id: root

    property string title
    property string subtitle
    property string ownColor
    property string picturePath
    /// Unix s; 0 = no time (contacts).
    property int timestamp: 0

    height: Math.max(avatar.height, subtitleLabel.y + subtitleLabel.height)
            + 2 * Theme.paddingMedium
    // Guard vs stray width binding; labels handle overflow.
    clip: true

    // Row height and title x use this id. Unresolved id throws: height never
    // runs, rows collapse.
    Avatar {
        id: avatar
        objectName: "resultAvatar"
        x: Theme.horizontalPageMargin
        y: Theme.paddingMedium
        initial: root.title
        ownColor: root.ownColor
        picturePath: root.picturePath
    }

    // Positioned off row, never title (as ChatListDelegate's time label): title
    // takes its width from this x. Anchoring this to title while title subtracted
    // this width = loop; long name ran off edge.
    Label {
        id: timeLabel
        objectName: "resultTime"
        x: root.width - width - Theme.horizontalPageMargin
        y: Theme.paddingMedium
        visible: root.timestamp > 0
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: Format.timeLabel(root.timestamp, Clock.now)
    }

    Label {
        id: titleLabel
        objectName: "resultTitle"
        x: avatar.x + avatar.width + Theme.paddingMedium
        y: Theme.paddingMedium
        width: (timeLabel.visible ? timeLabel.x : root.width - Theme.horizontalPageMargin)
               - x - Theme.paddingMedium
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        text: root.title
    }

    Label {
        id: subtitleLabel
        objectName: "resultSubtitle"
        x: titleLabel.x
        y: titleLabel.y + titleLabel.height
        width: root.width - x - Theme.horizontalPageMargin
        // Two lines then fade: one line of a sentence can't tell hits apart.
        wrapMode: Text.Wrap
        maximumLineCount: 2
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: root.subtitle
    }
}
