import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * A page header for a title the other end chose.
 *
 * Silica's PageHeader draws its title in a Label of its own and offers no
 * textFormat, so a chat named `<img src="https://tracker/p.gif">` would
 * be markup to it, and drawing the header would fetch the image -- from
 * an app whose whole point is that the network cannot watch. This is the
 * same header, laid out as PageHeader lays out its own: the title on the
 * line the page indicator sits on, right-aligned, no wider than the room
 * it has, in the page's own colour when the header leads somewhere and
 * the highlight when it does not -- with its one label pinned to plain
 * text.
 *
 * A second line under it, where PageHeader puts its description: what a
 * group's header says about itself, "7 members". The header keeps
 * PageHeader's height whatever is in it -- what sits below starts where
 * it starts -- so the line is drawn only while the two fit inside that
 * height, and a reader whose fonts are big enough to fill it with the
 * name alone keeps the name rather than getting a header over the first
 * message.
 */
Item {
    id: root

    /// What the header says. Shown as written, whatever it looks like.
    property alias title: titleLabel.text

    /// The line under the title, empty for none. Shown as written too:
    /// it sits beside a name the other end chose and is drawn by the
    /// same rules.
    property alias subtitle: subtitleLabel.text

    /// Whether the header leads to another page. Drawn as PageHeader
    /// draws a header of a page that can navigate forward.
    property bool interactive: false

    /// Whether the second line is drawn: only when there is one and the
    /// header has the room for it.
    readonly property bool showsSubtitle:
        subtitleLabel.text.length > 0
        && titleLabel.height + subtitleLabel.height <= root.height

    /// The header was tapped. What that opens is the page's to decide.
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
        // No wider than its text, and no wider than the page less its
        // margins.
        width: Math.min(implicitWidth, root.width - 2 * Theme.horizontalPageMargin)
        // The line PageHeader puts its first line on: it measures one
        // line of its font, and a single-line label is that tall. With a
        // second line under it the pair is centred instead, so the block
        // sits where the title alone would.
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
        // The quieter of the pair the title is drawn from, so the two
        // read as one header rather than two things.
        color: root.interactive ? Theme.secondaryColor
                                : Theme.secondaryHighlightColor
        font.pixelSize: Theme.fontSizeExtraSmall
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
    }
}
