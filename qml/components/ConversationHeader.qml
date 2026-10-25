import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Emoji.js" as Emoji

/*
 * PageHeader layout, PlainText label, for peer-chosen titles: PageHeader lacks
 * textFormat, so `<img src=...>` in a name would fetch (a network tell). Emoji:
 * escaped StyledText, Twemoji (Emoji.js). Keeps PageHeader's height; second
 * line (e.g. "7 members") only if it fits.
 */
Item {
    id: root

    /// Shown as written, emoji as pictures.
    property string title

    /// Line under title, as written; empty none.
    property alias subtitle: subtitleLabel.text

    /// Drawn like PageHeader's forward header.
    property bool interactive: false

    /// Title with emoji pictures, or "" for plain.
    readonly property string emojiTitle: Emoji.inText(root.title, false,
                                                      titleLabel.font.pixelSize,
                                                      Emoji.nameTint(titleLabel.color))

    /// Second line drawn: set, and room for it.
    readonly property bool showsSubtitle:
        subtitleLabel.text.length > 0
        && titleLabel.height + subtitleLabel.height <= root.height

    /// Page decides what opens.
    signal clicked()

    width: parent ? parent.width : 0
    height: Theme.itemSizeLarge

    MouseArea {
        objectName: "headerTap"
        anchors.fill: parent
        onClicked: root.clicked()
    }

    Label {
        id: titleLabel
        objectName: "headerTitle"
        // No wider than text or page less margins.
        width: Math.min(implicitWidth, root.width - 2 * Theme.horizontalPageMargin)
        // PageHeader's first-line spot; with subtitle, pair centred there.
        y: root.showsSubtitle
           ? Math.floor((root.height - height - subtitleLabel.height) / 2)
           : Math.floor((root.height - height) / 2)
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        horizontalAlignment: Text.AlignRight
        color: root.interactive ? Theme.primaryColor : Theme.highlightColor
        font.pixelSize: Theme.fontSizeLarge
        font.family: Theme.fontFamilyHeading
        truncationMode: TruncationMode.Fade
        textFormat: root.emojiTitle.length > 0 ? Text.StyledText : Text.PlainText
        text: root.emojiTitle.length > 0 ? root.emojiTitle : root.title
    }

    Label {
        id: subtitleLabel
        objectName: "headerSubtitle"
        visible: root.showsSubtitle
        width: Math.min(implicitWidth, root.width - 2 * Theme.horizontalPageMargin)
        y: titleLabel.y + titleLabel.height
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        horizontalAlignment: Text.AlignRight
        // Quieter of the pair, so header reads as one.
        color: root.interactive ? Theme.secondaryColor
                                : Theme.secondaryHighlightColor
        font.pixelSize: Theme.fontSizeExtraSmall
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
    }
}
