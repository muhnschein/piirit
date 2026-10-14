import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Format.js" as Format
import "."

/*
 * Chat-list row. Peer-sent name, preview: PlainText, see MessageDelegate.qml.
 * Own component so it loads and measures alone; bindings, not positioner (see
 * MessageDelegate.qml).
 */
Item {
    id: root

    property string chatName
    property string preview
    // Last sender; shown in group chats.
    property string previewSender
    property int unreadCount: 0
    // Unix seconds.
    property double lastUpdated: 0
    property bool isEncrypted: true
    property bool isPinned: false
    property bool isMuted: false
    property bool isContactRequest: false
    // Core's chat colour.
    property string chatColor
    property string avatarPath
    // DC_STATE_* of last message; >= 20 means we sent it.
    property int summaryState: 0

    readonly property bool summaryIsOurs: summaryState >= 20

    height: Math.max(avatar.height, previewLabel.y + previewLabel.height)
            + 2 * Theme.paddingMedium

    // Picture, or colour with initial.
    Avatar {
        id: avatar
        objectName: "avatar"
        x: Theme.horizontalPageMargin
        y: Theme.paddingMedium
        initial: root.chatName
        ownColor: root.chatColor
        picturePath: root.avatarPath
    }

    // Mute mark, time, pin mark on right: name stays plain.
    Row {
        id: marks
        x: root.width - width - Theme.horizontalPageMargin
        y: Theme.paddingMedium
        spacing: Theme.paddingSmall

        Label {
            objectName: "muteMark"
            visible: root.isMuted
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: "🔇"
        }

        Label {
            id: timeLabelItem
            objectName: "timeLabel"
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: Format.timeLabel(root.lastUpdated, Clock.now)
        }

        Label {
            objectName: "pinMark"
            visible: root.isPinned
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: "📌"
        }
    }

    Label {
        id: nameLabel
        objectName: "nameLabel"
        x: avatar.x + avatar.width + Theme.paddingMedium
        y: Theme.paddingMedium
        width: marks.x - x - Theme.paddingMedium
        truncationMode: TruncationMode.Fade
        textFormat: Text.PlainText
        // Mail icon: not encryptable. Pin/mute right: placement, not name.
        text: (root.isEncrypted ? "" : "✉ ") + root.chatName
    }

    Label {
        id: previewLabel
        objectName: "previewLabel"
        x: nameLabel.x
        y: nameLabel.y + nameLabel.height
        width: badge.x - x - Theme.paddingMedium
        truncationMode: TruncationMode.Fade
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: Text.PlainText
        // Ours: delivery mark. Others: sender name, if set.
        text: (root.summaryIsOurs ? Format.stateMark(root.summaryState) + " " : "")
              + (root.previewSender.length > 0 && !root.summaryIsOurs
                 ? root.previewSender + ": " : "")
              + root.preview
    }

    // Unread count; muted still counted, dimmed.
    Rectangle {
        id: badge
        objectName: "unreadBadge"
        visible: root.unreadCount > 0
        width: visible ? Math.max(height, badgeLabel.implicitWidth + Theme.paddingMedium) : 0
        height: visible ? badgeLabel.implicitHeight + Theme.paddingSmall : 0
        x: root.width - width - Theme.horizontalPageMargin
        y: previewLabel.y
        radius: height / 2
        color: root.isMuted ? Theme.rgba(Theme.secondaryColor, 0.4)
                            : Theme.highlightColor

        Label {
            id: badgeLabel
            objectName: "unreadLabel"
            anchors.centerIn: parent
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.primaryColor
            text: root.unreadCount > 99 ? "99+" : root.unreadCount
        }
    }
}
