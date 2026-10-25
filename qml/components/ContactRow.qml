import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Marks.js" as Marks

/*
 * Contact row, looks as chat-list row. Peer-supplied name, address:
 * PlainText, see MessageDelegate.qml; or escaped StyledText with search marks
 * (Marks.js).
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
    readonly property string nameText: (root.isKeyContact ? "" : "✉ ") + root.displayName
    /// Marked StyledText, or "" for plain.
    readonly property string markedName: Marks.marked(root.nameText, root.match,
                                                      Theme.highlightColor)
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
        textFormat: root.markedName.length > 0 ? Text.StyledText : Text.PlainText
        text: root.markedName.length > 0 ? root.markedName : root.nameText
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
