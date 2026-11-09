import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media
import Piirit 1.0

/*
 * One-to-one chat's contact. Swipe left from conversation, as group page. Read
 * first: picture with facts beside it, their status, call buttons, rows to what
 * the chat holds and how it behaves. Edits and actions in pulley. No address:
 * core's contact words only.
 *
 * Same ChatInfo as group page; one member, the contact. Row read into
 * properties: reload after a change rebuilds nothing on screen.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Name shown until core answers.
    property string chatName
    property string errorMessage: ""

    ChatInfo {
        id: chat
        objectName: "chat"
        account_id: page.accountId
        chat_id: page.chatId
        onError: page.errorMessage = message
        onClosed: page.leaveChat()
    }

    Connections {
        target: core
        // Name/picture changes arrive via contact; model reloads.
        onCore_event: {
            chat.handle_event(context_id, kind, payload_json)
            commonChats.handle_event(context_id, kind, payload_json)
        }
        onStatus_changed: {
            if (core.status === "ready") {
                chat.reload()
                commonChats.reload()
            }
        }
    }

    // Loaded on first show: most 1:1 chats are never opened. Contact before account: account
    // alone lists every chat. Ids up to 9 (self, devices) have none.
    ChatList {
        id: commonChats
        objectName: "commonChats"
        onError: page.errorMessage = message
    }

    function loadCommonChats() {
        var shown = page.status === PageStatus.Activating || page.status === PageStatus.Active
        if (shown && page.contactId > 9 && commonChats.contact_id === 0) {
            commonChats.contact_id = page.contactId
            commonChats.account_id = page.accountId
        }
    }

    onContactIdChanged: page.loadCommonChats()
    onStatusChanged: page.loadCommonChats()

    /// The one member, lifted from row.
    property int contactId: 0
    /// Given name, empty if none.
    property string givenName: ""
    /// Name they chose.
    property string ownName: ""
    /// Name shown: given, else theirs.
    property string displayName: ""
    property string contactColor: ""
    property string avatarPath: ""
    /// Their status line.
    property string statusLine: ""
    property bool isKeyContact: true
    /// Not seen long, or never.
    property bool seenLongAgo: false

    // Chat gone (deleted, contact blocked): conversation below goes too. Confirm or
    // countdown may still run; Silica drops stack ops mid-transition: finish it.
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

    function openMedia(kind) {
        if (kind === "map") {
            map.show(page.chatId, 0, 0)
        } else if (kind === "calls") {
            pageStack.push(Qt.resolvedUrl("CallsPage.qml"), {
                accountId: page.accountId,
                chatId: page.chatId,
                contactName: page.displayName
            })
        } else {
            pageStack.push(Qt.resolvedUrl("ChatMediaPage.qml"), {
                accountId: page.accountId,
                chatId: page.chatId,
                kind: kind
            })
        }
    }

    // Own chat: back to it, also from the all-chats page.
    function openChat(chatId, chatName) {
        if (chatId === page.chatId) {
            if (pageStack.currentPage === page) {
                pageStack.navigateBack()
            } else {
                pageStack.pop(pageStack.previousPage(page))
            }
            return
        }
        // Via the list page: it tracks the open chat for notifications.
        if (typeof appWindow !== "undefined" && appWindow.chatList !== null
                && appWindow.chatList.accountId === page.accountId) {
            appWindow.chatList.openChatAbove(chatId, chatName)
            return
        }
        pageStack.push(Qt.resolvedUrl("ConversationPage.qml"), {
            accountId: page.accountId,
            chatId: chatId,
            chatName: chatName
        })
    }

    /// Call them, or bring existing call to front. One call at a time.
    function placeCall(video) {
        if (typeof appWindow === "undefined") {
            return
        }
        if (!appWindow.placeCall(page.accountId, page.chatId, video === true)) {
            appWindow.showCall()
        }
    }

    function showEncryption() {
        pageStack.push(Qt.resolvedUrl("EncryptionPage.qml"), {
            accountId: page.accountId,
            contactId: page.contactId,
            name: page.displayName
        })
    }

    function editName() {
        pageStack.push(Qt.resolvedUrl("EditNamePage.qml"), {
            chat: chat,
            contactId: page.contactId,
            givenName: page.givenName,
            ownName: page.ownName,
            displayName: page.displayName
        })
    }

    function confirmBlock() {
        var dialog = pageStack.push(Qt.resolvedUrl("BlockContactDialog.qml"), {
            contactName: page.displayName,
            blocking: true
        })
        if (dialog) {
            dialog.accepted.connect(function() {
                chat.block_contact(page.contactId)
            })
        }
    }

    // Silica's countdown, as chat list's delete; core's delete after it.
    RemorsePopup {
        id: deleteRemorse
        objectName: "deleteRemorse"
    }

    function deleteChat() {
        //: Silica's countdown over the contact page, before the chat with
        //: them and its messages are deleted from this device.
        deleteRemorse.execute(qsTr("Deleting chat"), function() { chat.delete_chat() })
    }

    MapOpener {
        id: map
        objectName: "map"
        account_id: page.accountId
        chatName: page.chatName
        onError: page.errorMessage = message
    }

    // Picture, full size. None if no picture: initial disc is not one.
    function showPicture() {
        if (page.avatarPath.length === 0) {
            return
        }
        pageStack.push(Qt.resolvedUrl("PicturePage.qml"), {
            fileUrl: Media.fileUrl(page.avatarPath),
            fileName: ""
        })
    }

    // The contact's one row, read into page (Repeater is how QML reads rows).
    // Draws nothing: reload after save rebuilds nothing on screen.
    Repeater {
        model: chat.members

        delegate: Item {
            objectName: "contactDetails"
            width: 0
            height: 0

            Binding { target: page; property: "contactId"; value: model.contact_id }
            Binding { target: page; property: "givenName"; value: model.name }
            Binding { target: page; property: "ownName"; value: model.auth_name }
            Binding { target: page; property: "displayName"; value: model.display_name }
            Binding { target: page; property: "contactColor"; value: model.color }
            Binding { target: page; property: "avatarPath"; value: model.avatar_path }
            Binding { target: page; property: "statusLine"; value: model.status }
            Binding { target: page; property: "isKeyContact"; value: model.is_key_contact }
            Binding { target: page; property: "seenLongAgo"; value: model.seen_long_ago }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        // Farthest from finger: undone only by hand (Delete, Block).
        PullDownMenu {
            objectName: "contactMenu"
            visible: page.contactId > 0

            MenuItem {
                objectName: "deleteChat"
                //: Pulley on a contact's page: deletes the chat with them from this device.
                text: qsTr("Delete chat")
                onClicked: page.deleteChat()
            }
            MenuItem {
                objectName: "blockContact"
                text: qsTr("Block contact")
                onClicked: page.confirmBlock()
            }
            MenuItem {
                objectName: "encryptionInfo"
                //: Pulley on a contact's or group's page: opens the page with
                //: the keys' fingerprints.
                text: qsTr("Show encryption info")
                onClicked: page.showEncryption()
            }
            MenuItem {
                objectName: "editName"
                //: Pulley on a contact's page: opens the page to give them a nickname.
                text: qsTr("Edit name")
                onClicked: page.editName()
            }
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Contact")
            }

            IdentityRow {
                initial: page.displayName.length > 0 ? page.displayName : page.chatName
                ownColor: page.contactColor
                picturePath: page.avatarPath
                onPictureClicked: page.showPicture()

                NameLabel {
                    objectName: "nameLabel"
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeLarge
                    color: Theme.highlightColor
                    name: page.displayName.length > 0 ? page.displayName : page.chatName
                }

                // Their own name, under the one given here; only when it differs.
                Label {
                    objectName: "ownNameLabel"
                    visible: page.givenName.length > 0 && page.ownName.length > 0
                             && page.givenName !== page.ownName
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.secondaryHighlightColor
                    textFormat: Text.PlainText
                    //: Under a nickname on a contact's page. %1 is the name
                    //: the contact chose for themselves.
                    text: qsTr("Calls themselves %1").arg(page.ownName)
                }

                // Tap: keys to compare.
                Label {
                    objectName: "encryptionLabel"
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeSmall
                    color: encryptionTap.pressed ? Theme.highlightColor
                                                 : Theme.secondaryHighlightColor
                    // Translated literals pinned: binding reads page to pick one.
                    textFormat: Text.PlainText
                    text: page.isKeyContact
                          ? qsTr("End-to-end encrypted")
                          : qsTr("Not encrypted: a plain email contact")

                    MouseArea {
                        id: encryptionTap
                        objectName: "encryptionTap"
                        anchors.fill: parent
                        enabled: page.isKeyContact && page.contactId > 0
                        onClicked: page.showEncryption()
                    }
                }

                // Core asks: long-unseen contact may not read soon. Delta Chat's words, for
                // translations.
                Label {
                    objectName: "seenLabel"
                    visible: page.seenLongAgo
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.secondaryHighlightColor
                    //: Under a contact's name: the core has not seen them for
                    //: a long time, or never.
                    text: qsTr("Seen a long time ago")
                }
            }

            // Their own words about themselves, full width: can run long.
            ClampedText {
                objectName: "statusLabel"
                visible: page.statusLine.length > 0
                color: Theme.primaryColor
                text: page.statusLine
            }

            // Two buttons, one width each. Bindings, not Row: polish never runs headless.
            Item {
                objectName: "callButtons"
                visible: chat.can_call
                width: parent.width
                height: visible ? callButton.height : 0

                Button {
                    id: callButton
                    objectName: "callButton"
                    x: Theme.horizontalPageMargin
                    width: (parent.width - 2 * Theme.horizontalPageMargin - Theme.paddingLarge) / 2
                    icon.source: "image://theme/icon-m-call"
                    //: Button on a contact's page: starts a voice call.
                    text: qsTr("Call")
                    onClicked: page.placeCall(false)
                }
                Button {
                    objectName: "videoCallButton"
                    x: callButton.x + callButton.width + Theme.paddingLarge
                    width: callButton.width
                    icon.source: "image://theme/icon-m-video"
                    //: Button on a contact's page: starts a call with camera on.
                    text: qsTr("Video call")
                    onClicked: page.placeCall(true)
                }
            }

            SectionHeader {
                //: Over the rows to a chat's pictures, files, apps, calls and map.
                text: qsTr("In this chat")
            }

            // `=== true`: dconf gives undefined before read.
            DoorGrid {
                objectName: "doors"
                kinds: Media.doors(Settings.webxdcEnabled === true, chat.can_call,
                                   Settings.locationStreamingEnabled === true)
                onOpened: page.openMedia(kind)
            }

            SectionHeader {
                //: Over a chat's notifications and disappearing messages.
                text: qsTr("Settings")
            }

            ChatNotifications {
                objectName: "notifications"
                muted: chat.is_muted
                onPicked: chat.set_mute_duration(seconds)
            }

            DisappearingMessages {
                objectName: "disappearing"
                seconds: chat.ephemeral_timer
                canChange: chat.can_send
                onChosen: chat.set_ephemeral_timer(seconds)
            }

            SectionHeader {
                objectName: "commonChatsHeader"
                visible: commonChats.count > 0
                //: Over the row to the chats a contact is in too. Delta
                //: Chat's "Chats in Common".
                text: qsTr("Chats in common")
            }

            // One row however many: three faces, count, first names; list behind it.
            BackgroundItem {
                id: commonRow
                objectName: "commonChatsRow"
                visible: commonChats.count > 0
                width: parent.width
                height: Theme.itemSizeMedium
                onClicked: pageStack.push(Qt.resolvedUrl("CommonChatsPage.qml"), {
                    chats: commonChats,
                    contactPage: page
                })

                /// First chats' names, ", "-joined; "…" when more. Counts from `faces`: runs while
                /// model inserts, `commonChats.count` then reads a borrowed model.
                readonly property string names: {
                    var shown = []
                    for (var i = 0; i < faces.count && i < 3; i++) {
                        var face = faces.itemAt(i)
                        if (face) {
                            shown.push(face.chatName)
                        }
                    }
                    return shown.join(", ") + (faces.count > 3 ? ", …" : "")
                }
                readonly property real faceSize: Theme.itemSizeExtraSmall
                readonly property int faceCount: Math.min(3, faces.count)

                Repeater {
                    id: faces
                    model: commonChats.rows

                    Avatar {
                        property string chatName: model.name
                        visible: index < 3
                        // Later faces under earlier ones, a third showing.
                        z: 3 - index
                        x: Theme.horizontalPageMargin + index * commonRow.faceSize * 2 / 3
                        anchors.verticalCenter: parent.verticalCenter
                        width: commonRow.faceSize
                        initial: model.name
                        ownColor: model.color
                        // Hidden ones load nothing.
                        picturePath: index < 3 ? model.avatar_path : ""
                    }
                }

                Label {
                    id: commonCount
                    objectName: "commonChatsCount"
                    x: Theme.horizontalPageMargin + commonRow.faceSize
                       + (commonRow.faceCount - 1) * commonRow.faceSize * 2 / 3
                       + Theme.paddingLarge
                    width: parent.width - x - Theme.horizontalPageMargin
                    y: (parent.height - height - commonNames.height) / 2
                    truncationMode: TruncationMode.Fade
                    color: commonRow.highlighted ? Theme.highlightColor : Theme.primaryColor
                    //: Row on a contact's page; %n is how many chats they are in too.
                    text: qsTr("%n chat(s)", "", commonChats.count)
                }

                NameLabel {
                    id: commonNames
                    objectName: "commonChatsNames"
                    x: commonCount.x
                    y: commonCount.y + commonCount.height
                    width: commonCount.width
                    truncationMode: TruncationMode.Fade
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: commonRow.highlighted ? Theme.secondaryHighlightColor
                                                 : Theme.secondaryColor
                    name: commonRow.names
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
