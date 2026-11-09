import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media
import Piirit 1.0

/*
 * Group or channel: swipe left from conversation or tap header. Read first, as
 * contact page: picture with name and member count beside it, description, Invite
 * and Add members, three members and a row to all, rows to what the chat holds and
 * how it behaves. Name, description, picture on Edit group (pulley). Core owns
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
        } else if (kind === "media") {
            pageStack.push(Qt.resolvedUrl("ChatMediaPage.qml"), {
                accountId: page.accountId,
                chatId: page.chatId,
                kinds: ["gallery", "audio", "files"]
            })
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

    /// Own channel: invite only way subscribers join.
    readonly property bool offersInvite: (chat.can_edit && !chat.is_channel) || chat.is_own_channel
    /// Core takes no members added to a channel.
    readonly property bool offersAdd: chat.can_edit && !chat.is_channel

    function addMembers() {
        pageStack.push(Qt.resolvedUrl("AddMembersPage.qml"), {
            accountId: page.accountId,
            chat: chat
        })
    }

    function invite() {
        pageStack.push(Qt.resolvedUrl("QrPage.qml"), {
            accountId: page.accountId,
            chatId: page.chatId,
            chatName: chat.name,
            isChannel: chat.is_channel
        })
    }

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

    /// Tap on member: their page, or menu. See MemberOpener.
    MemberOpener {
        id: memberOpener
        chatInfo: chat
        accountId: page.accountId
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        // Destructive farthest from finger. Left: Delete chat in Leave's place.
        PullDownMenu {
            objectName: "groupMenu"
            visible: chat.loaded

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
                objectName: "encryptionInfo"
                visible: chat.is_encrypted
                text: qsTr("Show encryption info")
                onClicked: page.showEncryption()
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

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                objectName: "pageHeader"
                //: Delta Chat's "channel". Heading of a channel's page.
                title: chat.is_channel ? qsTr("Channel") : qsTr("Group")
            }

            IdentityRow {
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

                Label {
                    objectName: "memberCount"
                    visible: chat.loaded
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.secondaryHighlightColor
                    //: Under a group's name on its page. %n is how many people
                    //: are in it.
                    text: qsTr("%n member(s)", "", chat.member_count)
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
                objectName: "descriptionLabel"
                visible: chat.description.length > 0
                color: Theme.primaryColor
                text: chat.description
            }

            // The two ways a group grows. Bindings, not Row: polish never runs headless.
            Item {
                objectName: "growButtons"
                visible: page.offersInvite || page.offersAdd
                width: parent.width
                height: visible ? inviteButton.height : 0

                readonly property real half: (width - 2 * Theme.horizontalPageMargin
                                              - Theme.paddingLarge) / 2

                Button {
                    id: inviteButton
                    objectName: "inviteButton"
                    visible: page.offersInvite
                    x: Theme.horizontalPageMargin
                    width: page.offersAdd ? parent.half : 2 * parent.half + Theme.paddingLarge
                    //: Button on a group's page: opens the QR code others scan to join.
                    text: qsTr("Invite")
                    onClicked: page.invite()
                }
                Button {
                    id: addButton
                    objectName: "addMembersButton"
                    visible: page.offersAdd
                    x: page.offersInvite ? inviteButton.x + inviteButton.width + Theme.paddingLarge
                                         : Theme.horizontalPageMargin
                    width: page.offersInvite ? parent.half : 2 * parent.half + Theme.paddingLarge
                    text: qsTr("Add members")
                    onClicked: page.addMembers()
                }
            }

            SectionHeader {
                objectName: "membersHeader"
                visible: chat.member_count > 0
                //: Over the first few members on a group's page.
                text: qsTr("Members")
            }

            // Three, not all: page keeps its length with 4 or 400 members.
            Repeater {
                model: chat.preview

                MemberRow {
                    objectName: "memberRow" + modelData.contact_id
                    width: column.width
                    contactId: modelData.contact_id
                    displayName: modelData.display_name
                    ownColor: modelData.color
                    picturePath: modelData.avatar_path
                    isKeyContact: modelData.is_key_contact
                    opener: memberOpener
                }
            }

            BackgroundItem {
                objectName: "allMembers"
                visible: chat.member_count > 0
                width: parent.width
                height: Theme.itemSizeSmall
                onClicked: page.showMembers()

                Label {
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    truncationMode: TruncationMode.Fade
                    color: parent.highlighted ? Theme.highlightColor : Theme.primaryColor
                    //: Row under the first members on a group's page: opens the
                    //: list of all. %n is how many.
                    text: qsTr("All %n member(s)", "", chat.member_count)
                }
            }

            SectionHeader {
                text: qsTr("In this chat")
            }

            DoorRow {
                objectName: "mediaRow"
                kind: "media"
                onClicked: page.openMedia("media")
            }
            // `=== true`: dconf gives `undefined` until key read.
            DoorRow {
                objectName: "appsRow"
                visible: Settings.webxdcEnabled === true
                kind: "apps"
                onClicked: page.openMedia("apps")
            }
            DoorRow {
                objectName: "mapRow"
                visible: Settings.locationStreamingEnabled === true
                kind: "map"
                onClicked: page.openMedia("map")
            }

            SectionHeader {
                text: qsTr("Settings")
            }

            ChatNotifications {
                objectName: "notifications"
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
