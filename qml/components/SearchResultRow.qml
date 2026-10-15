import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Format.js" as Format
import "."

/*
 * Search result, laid out as chat-list row. Others' text: PlainText (MessageDelegate.qml).
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

    // Height and title x use this id; unresolved, rows collapse.
    Avatar {
        id: avatar
        objectName: "resultAvatar"
        x: Theme.horizontalPageMargin
        y: Theme.paddingMedium
        initial: root.title
        ownColor: root.ownColor
        picturePath: root.picturePath
    }

    // Off row, not title (as ChatListDelegate's time): title width uses this x, so
    // anchoring to title looped and long names ran off edge.
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
        // Two lines: one can't tell hits apart.
        wrapMode: Text.Wrap
        maximumLineCount: 2
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: root.subtitle
    }
}
