import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Page header for a peer-chosen title. Silica's PageHeader has no textFormat:
 * `<img src=...>` in a chat name would fetch the image (network watching is
 * what this app avoids). Same layout as PageHeader, label pinned to PlainText.
 *
 * Header keeps PageHeader's height whatever it holds. Second line
 * (description, e.g. "7 members") drawn only if it fits; with big fonts the name
 * alone keeps it, not a header over the first message.
 */
Item {
    id: root

    /// Header text. Shown as written.
    property alias title: titleLabel.text

    /// Line under title; empty: none. Shown as written.
    property alias subtitle: subtitleLabel.text

    /// Leads to another page: drawn like PageHeader's forward header.
    property bool interactive: false

    /// Second line drawn: set, and room for it.
    readonly property bool showsSubtitle:
        subtitleLabel.text.length > 0
        && titleLabel.height + subtitleLabel.height <= root.height

    /// Header tapped. Page decides what opens.
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
        // PageHeader's first-line spot. With subtitle, pair centred on same spot.
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
        textFormat: Text.PlainText
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
