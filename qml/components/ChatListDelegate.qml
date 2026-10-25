import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Emoji.js" as Emoji
import "../js/Format.js" as Format
import "../js/Marks.js" as Marks
import "."

/*
 * Chat-list row. Peer-sent name, preview: PlainText (MessageDelegate.qml); with
 * search marks (Marks.js) or emoji: escaped StyledText, Twemoji (Emoji.js),
 * preview's grey as its text. Own component, loads and measures alone;
 * bindings, not positioner (ditto).
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

    /// List's search text; marked in name, core's only match field.
    property string match

    readonly property bool summaryIsOurs: summaryState >= 20
    /// Preview line as written. Ours: delivery mark. Others: sender name, if set.
    readonly property string previewText: (root.summaryIsOurs
                                           ? Format.stateMark(root.summaryState) + " " : "")
                                          + (root.previewSender.length > 0 && !root.summaryIsOurs
                                             ? root.previewSender + ": " : "")
                                          + root.preview
    /// Same with emoji pictures, grey as its text; "" for as written.
    readonly property string emojiPreview: Emoji.inText(root.previewText, false,
                                                        Theme.fontSizeExtraSmall,
                                                        Theme.secondaryColor)

    // Mail icon: unencryptable. Pin/mute on right: placement, not name.
    readonly property string mailMark: root.isEncrypted ? "" : "✉ "
    readonly property string nameText: root.mailMark + root.chatName
    /// Marked StyledText, or "" for plain. Name only: mark stays font's glyph.
    readonly property string markedName: Marks.marked(root.chatName, root.match,
                                                      Theme.highlightColor)
    /// Name as drawn, marks and emoji pictures, after mark; "" for plain.
    readonly property string drawnName: Emoji.drawn(root.chatName, root.markedName,
                                                    Theme.fontSizeMedium,
                                                    nameLabel.color)
    readonly property string shownName: root.drawnName.length > 0
                                        ? root.mailMark + root.drawnName : ""

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

    // Mute mark, time, pin mark on right: name plain.
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
        textFormat: root.shownName.length > 0 ? Text.StyledText : Text.PlainText
        text: root.shownName.length > 0 ? root.shownName : root.nameText
    }

    // Contact request: tagged before preview (Accept or Block waits in the chat).
    Rectangle {
        id: requestTag
        objectName: "requestTag"
        visible: root.isContactRequest
        x: nameLabel.x
        anchors.verticalCenter: previewLabel.verticalCenter
        width: visible ? requestLabel.implicitWidth + 2 * Theme.paddingMedium : 0
        height: requestLabel.implicitHeight
        radius: height / 2
        color: "transparent"
        border.color: Theme.highlightColor
        border.width: 1

        Label {
            id: requestLabel
            objectName: "requestLabel"
            anchors.centerIn: parent
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.highlightColor
            //: Tag in front of a chat-list row's preview when the chat is a
            //: contact request (someone new wrote, not yet accepted).
            text: qsTr("Request")
        }
    }

    Label {
        id: previewLabel
        objectName: "previewLabel"
        x: requestTag.visible ? requestTag.x + requestTag.width + Theme.paddingSmall
                              : nameLabel.x
        y: nameLabel.y + nameLabel.height
        width: badge.x - x - Theme.paddingMedium
        truncationMode: TruncationMode.Fade
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        textFormat: root.emojiPreview.length > 0 ? Text.StyledText : Text.PlainText
        text: root.emojiPreview.length > 0 ? root.emojiPreview : root.previewText
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
