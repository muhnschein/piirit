import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Format.js" as Format
import "."

/*
 * One search result, whatever kind it is. Laid out like a chat-list row so
 * the three groups read as one list: the same round avatar, the same
 * spacing, a second line for the detail.
 *
 * Titles and subtitles are names and message text the other end chose, so
 * both are pinned to PlainText -- see MessageDelegate.qml.
 */
Item {
    id: root

    property string title
    property string subtitle
    property string ownColor
    property string picturePath
    /// Unix seconds. 0 means the row has no time worth showing, which is
    /// what a contact result is: it has no moment attached to it.
    property int timestamp: 0

    height: Math.max(avatar.height, subtitleLabel.y + subtitleLabel.height)
            + 2 * Theme.paddingMedium
    // Nothing should reach past the edge even if a width binding is ever
    // wrong again; the labels below say what they do with the overflow.
    clip: true

    // The three references to this below -- the row's height, and the
    // title's x -- are why losing it took the whole row with it: an
    // unresolved id throws, the height binding never runs, and every row
    // collapsed to nothing under headings that still counted them.
    Avatar {
        id: avatar
        objectName: "resultAvatar"
        x: Theme.horizontalPageMargin
        y: Theme.paddingMedium
        initial: root.title
        ownColor: root.ownColor
        picturePath: root.picturePath
    }

    // Positioned off the row, never off the title. ChatListDelegate lays
    // its own time label out this way, and its name label takes its width
    // from this label's *x*. Anchoring this to the title while the title
    // subtracted this label's width closed a loop: neither width
    // resolved, so the title kept its natural one and a long chat name
    // ran off the right-hand edge.
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
        // Two lines, then faded. A message that matched a search is
        // usually a sentence, and one line of it says too little to tell
        // two hits apart.
        wrapMode: Text.Wrap
        maximumLineCount: 2
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: root.subtitle
    }
}
