import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Contact row, same look as chat-list row: avatar, colour, spacing.
 * Peer-supplied name and address: PlainText, see MessageDelegate.qml.
 */
Item {
    id: root

    property string displayName
    property string address
    /// Address under name. Off by default (means nothing to chatmail reader);
    /// profiles page shows it: reader's own, tells accounts apart.
    property bool showAddress: false
    property string ownColor
    property string picturePath
    /// Address contact: not encryptable.
    property bool isKeyContact: true
    /// Right space for overlays (badge); text fades first.
    property real trailingSpace: 0

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
        textFormat: Text.PlainText
        // Mail icon: not encryptable.
        text: (root.isKeyContact ? "" : "✉ ") + root.displayName
    }

    Label {
        id: addressLabel
        objectName: "contactAddress"
        visible: root.showAddress
        x: nameLabel.x
        y: nameLabel.y + nameLabel.height
        width: nameLabel.width
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: root.address
    }
}
