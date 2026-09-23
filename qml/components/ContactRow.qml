import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * One contact, laid out like a chat-list row: the same round avatar in the
 * contact's own colour, the same spacing. The two lists sit one tap apart,
 * and looked like different applications.
 *
 * Name and address are whatever the other end supplied, so both are pinned
 * to PlainText -- see MessageDelegate.qml.
 */
Item {
    id: root

    property string displayName
    property string address
    /// Whether to draw the address under the name. Off by default: an
    /// address means nothing to a reader of a chatmail app, and the one
    /// place it is worth showing is the profiles page, where it is the
    /// reader's own and tells two accounts apart.
    property bool showAddress: false
    property string ownColor
    property string picturePath
    /// An address contact cannot be written to encrypted.
    property bool isKeyContact: true
    /// Room kept free at the right, for whatever the page draws over the
    /// end of the row -- a badge, a mark -- so the text fades before it.
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
        // Centred on the avatar when it is the only line.
        y: root.showAddress ? Theme.paddingMedium
                            : avatar.y + (avatar.height - height) / 2
        width: root.width - x - Theme.horizontalPageMargin - root.trailingSpace
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        // The same mark the chat list uses: a mail icon for a contact
        // that cannot be encrypted to.
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
