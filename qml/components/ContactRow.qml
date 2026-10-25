import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Emoji.js" as Emoji
import "../js/Marks.js" as Marks

/*
 * Contact row, looks as chat-list row. Peer-supplied name, address:
 * PlainText, see MessageDelegate.qml; or escaped StyledText with search marks
 * (Marks.js), name's emoji as Twemoji (Emoji.js `drawn`).
 */
Item {
    id: root

    property string displayName
    property string address
    /// Address under name. Off by default (means nothing on chatmail); profiles
    /// page shows it to tell accounts apart.
    property bool showAddress: false
    property string ownColor
    property string picturePath
    /// Address contact: not encryptable.
    property bool isKeyContact: true
    /// Right space for overlays (badge); text fades first.
    property real trailingSpace: 0
    /// List's search text; occurrences marked.
    property string match

    // Mail icon: not encryptable.
    readonly property string mailMark: root.isKeyContact ? "" : "✉ "
    readonly property string nameText: root.mailMark + root.displayName
    /// Marked StyledText, or "" for plain. Name only: mark stays font's glyph.
    readonly property string markedName: Marks.marked(root.displayName, root.match,
                                                      Theme.highlightColor)
    /// Name as drawn, marks and emoji pictures, after mark; "" for plain.
    readonly property string drawnName: Emoji.drawn(root.displayName, root.markedName,
                                                    Theme.fontSizeMedium,
                                                    nameLabel.color, true)
    readonly property string shownName: root.drawnName.length > 0
                                        ? root.mailMark + root.drawnName : ""
    readonly property string markedAddress: Marks.marked(root.address, root.match,
                                                         Theme.highlightColor)

    height: Math.max(avatar.height,
                     (root.showAddress ? addressLabel.y + addressLabel.height
                                       : nameLabel.y + nameLabel.height))
            + 2 * Theme.paddingMedium

    Avatar {
        id: avatar
        objectName: "contactAvatar"
        x: Theme.horizontalPageMargin
        y: Theme.paddingMedium
        initial: root.displayName
        ownColor: root.ownColor
        picturePath: root.picturePath
    }

    Label {
        id: nameLabel
        objectName: "contactName"
        x: avatar.x + avatar.width + Theme.paddingMedium
        // Centred when single line.
        y: root.showAddress ? Theme.paddingMedium
                            : avatar.y + (avatar.height - height) / 2
        width: root.width - x - Theme.horizontalPageMargin - root.trailingSpace
        truncationMode: TruncationMode.Fade
        textFormat: root.shownName.length > 0 ? Text.StyledText : Text.PlainText
        text: root.shownName.length > 0 ? root.shownName : root.nameText
    }

    Label {
        id: addressLabel
        objectName: "contactAddress"
        visible: root.showAddress
        x: nameLabel.x
        y: nameLabel.y + nameLabel.height
        width: nameLabel.width
        truncationMode: TruncationMode.Fade
        textFormat: root.markedAddress.length > 0 ? Text.StyledText : Text.PlainText
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: root.markedAddress.length > 0 ? root.markedAddress : root.address
    }
}
