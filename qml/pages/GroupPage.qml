import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media
import Piirit 1.0

/*
 * Group or channel: swipe left from conversation or tap header. Read first, as
 * contact page: picture with name beside it, description, rows to what the chat
 * holds and how it behaves, members as one row (as Chats in common); Add members and
 * Invite on its page. Name, description, picture on Edit group (pulley). Core owns
 * data (ChatInfo); not joined: core refuses edits, entries hidden.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Name shown until core answers.
    property string chatName
    property string errorMessage: ""

    /// Renamed here (Edit group page).
    signal renamed(string name)

    ChatInfo {
        id: chat
        objectName: "chat"
        account_id: page.accountId
        chat_id: page.chatId
        onError: page.errorMessage = message
        onRenamed: page.renamed(name)
        // Left group still a chat: back goes to it, not list.
        // Confirm pop may still run; Silica drops stack ops mid-transition: finish it.
        onLeft: {
            if (pageStack.completeAnimation) {
                pageStack.completeAnimation()
            }
            pageStack.pop()
        }
        onClosed: page.leaveChat()
    }

    Connections {
        target: core
        // Rename/join from other devices too; model ignores other chats.
        onCore_event: chat.handle_event(context_id, kind, payload_json)
        // Model made before core up: nothing to load.
        onStatus_changed: {
            if (core.status === "ready") {
                chat.reload()
            }
        }
    }

    // Deleted: conversation below goes too. See ContactPage.leaveChat.
    function leaveChat() {
        if (pageStack.completeAnimation) {
            pageStack.completeAnimation()
        }
        var below = pageStack.previousPage(page)
        if (below && below.chatId === page.chatId) {
            below = pageStack.previousPage(below)
        }
        pageStack.pop(below)
    }

    // Picture large, on chat picture page. None if no picture (initial disc).
    function showPicture() {
        if (chat.avatar_path.length === 0) {
            return
        }
        pageStack.push(Qt.resolvedUrl("PicturePage.qml"), {
            fileUrl: Media.fileUrl(chat.avatar_path),
            fileName: ""
        })
    }

    function openMedia(kind) {
        if (kind === "map") {
            map.show(page.chatId, 0, 0)
        } else {
            pageStack.push(Qt.resolvedUrl("ChatMediaPage.qml"), {
                accountId: page.accountId,
                chatId: page.chatId,
                kind: kind
            })
        }
    }

    MapOpener {
        id: map
        objectName: "map"
        account_id: page.accountId
        chatName: page.chatName
        onError: page.errorMessage = message
    }

    /// Members page holds Add members and Invite: reachable while empty too (new own channel).
    readonly property bool offersGrowth: chat.can_edit || chat.is_own_channel

    function showMembers() {
        pageStack.push(Qt.resolvedUrl("MembersPage.qml"), {
            accountId: page.accountId,
            chat: chat
        })
    }

    function showEncryption() {
        pageStack.push(Qt.resolvedUrl("EncryptionPage.qml"), {
            accountId: page.accountId,
            chatId: page.chatId,
            name: chat.name
        })
    }

    function editGroup() {
        pageStack.push(Qt.resolvedUrl("EditGroupPage.qml"), { chat: chat })
    }

    // Own page with platform accept, not countdown: no way back unless re-added,
    // and pull-down countdown easy to miss.
    function confirmLeave() {
        var dialog = pageStack.push(Qt.resolvedUrl("LeaveGroupDialog.qml"),
                                    { groupName: chat.name })
        if (dialog) {
            dialog.accepted.connect(function() {
                chat.leave()
            })
        }
    }

    // Silica's countdown, as chat list's delete; core's delete after it.
    RemorsePopup {
        id: deleteRemorse
        objectName: "deleteRemorse"
    }

    function deleteChat() {
        deleteRemorse.execute(qsTr("Deleting chat"), function() { chat.delete_chat() })
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        // Destructive farthest from finger. Left: Delete chat in Leave's place.
        PullDownMenu {
            objectName: "groupMenu"
            visible: chat.loaded

            MenuItem {
                objectName: "encryptionInfo"
                visible: chat.is_encrypted
                text: qsTr("Show encryption info")
                onClicked: page.showEncryption()
            }
            MenuItem {
                objectName: "leaveButton"
                visible: chat.can_edit
                text: qsTr("Leave group")
                onClicked: page.confirmLeave()
            }
            MenuItem {
                objectName: "deleteChat"
                visible: !chat.can_edit
                text: qsTr("Delete chat")
                onClicked: page.deleteChat()
            }
            MenuItem {
                objectName: "editGroup"
                visible: chat.can_edit || chat.can_edit_description
                //: Pulley on a group's page: opens the page to change its name,
                //: description and picture. Delta Chat's "Edit Group".
                text: chat.is_channel ? qsTr("Edit channel") : qsTr("Edit group")
                onClicked: page.editGroup()
            }
        }

        HeaderBand {
            headerBottom: pageHeader.y + pageHeader.height
            pictureTop: identity.y + identity.pictureTop
            lastBottom: description.visible ? description.y + description.height
                        : identity.y + identity.height - identity.pictureTop
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                id: pageHeader
                objectName: "pageHeader"
                //: Delta Chat's "channel". Heading of a channel's page.
                title: chat.is_channel ? qsTr("Channel") : qsTr("Group")
            }

            IdentityRow {
                id: identity
                initial: chat.loaded ? chat.name : page.chatName
                ownColor: chat.color
                picturePath: chat.avatar_path
                onPictureClicked: page.showPicture()

                NameLabel {
                    objectName: "nameLabel"
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeLarge
                    color: Theme.highlightColor
                    name: chat.loaded ? chat.name : page.chatName
                }

                // Tap: members' keys.
                Label {
                    objectName: "encryptionLabel"
                    visible: chat.is_encrypted
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeSmall
                    color: encryptionTap.pressed ? Theme.highlightColor
                                                 : Theme.secondaryHighlightColor
                    text: qsTr("End-to-end encrypted")

                    MouseArea {
                        id: encryptionTap
                        objectName: "encryptionTap"
                        anchors.fill: parent
                        onClicked: page.showEncryption()
                    }
                }
            }

            // What the group is for, full width: can run long.
            ClampedText {
                id: description
                objectName: "descriptionLabel"
                visible: chat.description.length > 0
                color: Theme.primaryColor
                text: chat.description
            }

            SectionHeader {
                text: qsTr("In this chat")
            }

            // `=== true`: dconf gives `undefined` until key read.
            DoorGrid {
                objectName: "doors"
                kinds: Media.doors(Settings.webxdcEnabled === true, false,
                                   Settings.locationStreamingEnabled === true)
                onOpened: page.openMedia(kind)
            }

            SectionHeader {
                text: qsTr("Settings")
            }

            ChatNotifications {
                objectName: "notifications"
                width: parent.width
                accountId: page.accountId
                chatId: page.chatId
                chatName: chat.loaded ? chat.name : page.chatName
                chat: chat
                muted: chat.is_muted
                // Nobody replies to reader in a channel. `=== true`: dconf undefined till read.
                mentionsStillNotify: !chat.is_channel && Settings.notificationsEnabled === true
                                     && Settings.mentionNotifications === true
                onPicked: chat.set_mute_duration(seconds)
            }

            DisappearingMessages {
                objectName: "disappearing"
                seconds: chat.ephemeral_timer
                canChange: chat.can_send
                onChosen: chat.set_ephemeral_timer(seconds)
            }

            SectionHeader {
                objectName: "membersHeader"
                visible: chat.member_count > 0 || page.offersGrowth
                //: Over the row to a group's members.
                text: qsTr("Members")
            }

            // One row however many: three faces, count, first names; list behind it.
            BackgroundItem {
                id: membersRow
                objectName: "membersRow"
                visible: chat.member_count > 0 || page.offersGrowth
                width: parent.width
                height: Theme.itemSizeMedium
                onClicked: page.showMembers()

                readonly property real faceSize: Theme.itemSizeExtraSmall
                readonly property int faceCount: chat.preview.length

                Repeater {
                    model: chat.preview

                    Avatar {
                        // Later faces under earlier ones, a third showing.
                        z: 3 - index
                        x: Theme.horizontalPageMargin + index * membersRow.faceSize * 2 / 3
                        anchors.verticalCenter: parent.verticalCenter
                        width: membersRow.faceSize
                        initial: modelData.display_name
                        ownColor: modelData.color
                        picturePath: modelData.avatar_path
                    }
                }

                Label {
                    id: memberCount
                    objectName: "memberCount"
                    x: Theme.horizontalPageMargin + (membersRow.faceCount > 0
                        ? membersRow.faceSize
                          + (membersRow.faceCount - 1) * membersRow.faceSize * 2 / 3
                          + Theme.paddingLarge
                        : 0)
                    width: parent.width - x - Theme.horizontalPageMargin
                    y: (parent.height - height - memberNames.height) / 2
                    truncationMode: TruncationMode.Fade
                    color: membersRow.highlighted ? Theme.highlightColor : Theme.primaryColor
                    //: Row on a group's page, opens the list of all. %n is how many
                    //: people are in it.
                    text: qsTr("%n member(s)", "", chat.member_count)
                }

                // Shown ones' names; "…" when more.
                NameLabel {
                    id: memberNames
                    objectName: "memberNames"
                    x: memberCount.x
                    y: memberCount.y + memberCount.height
                    width: memberCount.width
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: membersRow.highlighted ? Theme.secondaryHighlightColor
                                                  : Theme.secondaryColor
                    name: chat.preview.map(function(member) { return member.display_name })
                              .join(", ")
                          + (chat.preview_more ? ", …" : "")
                }
            }

        }

        VerticalScrollDecorator {}
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }
}
