import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media
import Piirit 1.0

/*
 * Group page: picture, name, members; swipe left from conversation or tap
 * header. Core owns data (ChatInfo, get_full_chat_by_id); edits go at once:
 * name after typing pause and on leave, rest on tap. Not joined or not group:
 * core refuses edits, controls hidden.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Name shown until core answers.
    property string chatName

    /// Renamed here.
    signal renamed(string name)

    ChatInfo {
        id: chat
        objectName: "chat"
        account_id: page.accountId
        chat_id: page.chatId
        onError: {
            page.errorMessage = message
            // Not sent to core: reset to core's value.
            page.refillDescription()
        }
        // Filled from core, not while typing: reload would reset cursor.
        onLoaded_changed: {
            if (chat.loaded && !page.edited) {
                // Setting field fires onTextChanged, like typing.
                page.filling = true
                nameField.text = chat.name
                page.filling = false
            }
            if (chat.loaded && !page.descriptionEdited) {
                page.refillDescription()
            }
        }
        onSaved: notice.show(qsTr("Saved"))
        onRenamed: {
            notice.show(qsTr("Saved"))
            page.renamed(name)
        }
        // Left group still a chat: back goes to it, not list.
        // Confirm pop may still run; Silica drops stack ops mid-transition: finish it.
        onLeft: {
            if (pageStack.completeAnimation) {
                pageStack.completeAnimation()
            }
            pageStack.pop()
        }
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

    /// Typed since load. Guards refill.
    property bool edited: false
    /// Refill write, not an edit.
    property bool filling: false
    /// Description typed since save/load. Guards refill, like `edited`.
    property bool descriptionEdited: false
    property string errorMessage: ""

    // Pause, not keystroke: round trip per letter.
    Timer {
        id: autosave
        objectName: "autosave"
        interval: 1200
        onTriggered: page.applyEdits()
    }

    function applyEdits() {
        if (!chat.loaded || !page.edited) {
            return
        }
        page.edited = false
        // Group needs a name: blank reverts to current, not sent as empty.
        if (nameField.text.trim().length === 0) {
            page.filling = true
            nameField.text = chat.name
            page.filling = false
            return
        }
        chat.rename(nameField.text)
    }

    function refillDescription() {
        page.filling = true
        descriptionField.text = chat.description
        page.filling = false
        page.descriptionEdited = false
    }

    // Not on pause: each change messages all members. Saved on blur and leave.
    function applyDescription() {
        if (!chat.loaded || !page.descriptionEdited) {
            return
        }
        page.descriptionEdited = false
        chat.set_description(descriptionField.text)
    }

    function noteEdit() {
        if (chat.loaded && !page.filling) {
            page.edited = true
            autosave.restart()
        }
    }

    // Leave saves too (back-swipe in pause drops typing); pending removals go now.
    onStatusChanged: {
        page.showMembersOnceShown()
        if (status === PageStatus.Deactivating) {
            page.applyEdits()
            page.applyDescription()
            nameField.done()
            doomedMembers.flush()
        }
    }

    /// Builds member rows on first page-in (`memberSection`), while Activating, so
    /// they're there as page slides in.
    function showMembersOnceShown() {
        if (page.status === PageStatus.Activating
                || page.status === PageStatus.Active) {
            memberSection.active = true
        }
    }

    // Page made on screen never sees status change on way in.
    Component.onCompleted: page.showMembersOnceShown()

    /// Removal wait, ms. Tests lower it.
    property alias pendingDelay: doomedMembers.delay

    /// Members asked to remove, in undo window. Not Silica `remorseAction`: its row
    /// dies on reload. See PendingRemoval.
    PendingRemoval {
        id: doomedMembers
        onRemove: chat.remove_member(id)
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

    // Gallery by URL. Only Attach*Page files name `Sailfish.Pickers`: missing type
    // costs button, not page.
    function pickPicture() {
        var picker = pageStack.push(Qt.resolvedUrl("AttachPhotoPage.qml"))
        if (picker) {
            picker.picked.connect(function(path) {
                // Core copies file to blob dir; picked file may vanish.
                chat.set_picture(path)
            })
        }
    }

    // All chat content kinds; tile says kind.
    function openMedia(kind) {
        if (kind === "map") {
            map.show(page.chatId, 0, 0)
            return
        }
        pageStack.push(Qt.resolvedUrl("ChatMediaPage.qml"), {
            accountId: page.accountId,
            chatId: page.chatId,
            kind: kind
        })
    }

    MapOpener {
        id: map
        objectName: "map"
        account_id: page.accountId
        chatName: page.chatName
        onError: page.errorMessage = message
    }

    function addMembers() {
        pageStack.push(Qt.resolvedUrl("AddMembersPage.qml"), {
            accountId: page.accountId,
            chat: chat
        })
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

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        // Both entries for members: hidden for left/removed reader (empty pull-down). Own channel:
        // its invite, only way subscribers join (core takes no members added).
        PullDownMenu {
            objectName: "groupMenu"
            visible: chat.can_edit || chat.is_own_channel

            MenuItem {
                objectName: "channelInvite"
                visible: chat.is_own_channel
                //: Delta Chat's "qrshow_title". Opens the code others scan to join the channel.
                text: qsTr("QR invite code")
                onClicked: pageStack.push(Qt.resolvedUrl("QrPage.qml"), {
                    accountId: page.accountId,
                    chatId: page.chatId,
                    chatName: chat.name
                })
            }

            MenuItem {
                objectName: "leaveButton"
                visible: chat.can_edit
                text: qsTr("Leave group")
                onClicked: page.confirmLeave()
            }
            // Only when there is a picture to remove.
            MenuItem {
                objectName: "removePicture"
                visible: chat.can_edit && chat.avatar_path.length > 0
                text: qsTr("Remove picture")
                onClicked: chat.clear_picture()
            }
            // Here, not on picture (tap enlarges), as contact page.
            MenuItem {
                objectName: "changePicture"
                visible: chat.can_edit
                //: Pull-down entry on a group's page: opens the gallery to
                //: pick a new picture for the group.
                text: qsTr("Change picture")
                onClicked: page.pickPicture()
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

            // Tap enlarges; change via pull-down.
            Item {
                width: parent.width
                height: bigAvatar.height + 2 * Theme.paddingLarge

                Avatar {
                    id: bigAvatar
                    objectName: "groupAvatar"
                    anchors.centerIn: parent
                    width: 2 * Theme.itemSizeExtraLarge
                    initial: chat.loaded ? chat.name : page.chatName
                    ownColor: chat.color
                    picturePath: chat.avatar_path
                }

                MouseArea {
                    objectName: "pictureTap"
                    anchors.fill: bigAvatar
                    enabled: chat.avatar_path.length > 0
                    onClicked: page.showPicture()
                }
            }

            // Name field while joined. See EditableName.qml.
            EditableName {
                id: nameField
                objectName: "groupNameControl"
                fieldObjectName: "groupNameField"
                hintObjectName: "nameHint"
                placeholderText: qsTr("Group name")
                hint: qsTr("Everyone in the group sees the name")
                canEdit: chat.can_edit
                onTextChanged: page.noteEdit()
            }

            // Description: shown to all, editable where core allows, keeps line breaks.
            // Hidden if empty and not editable.
            TextArea {
                id: descriptionField
                objectName: "descriptionField"
                visible: chat.can_edit_description || chat.description.length > 0
                width: parent.width
                readOnly: !chat.can_edit_description
                //: Label of the text that says what a group or channel is
                //: for, on its info page, and the prompt in its empty field.
                label: qsTr("Description")
                placeholderText: label
                onTextChanged: {
                    if (chat.loaded && !page.filling) {
                        page.descriptionEdited = true
                    }
                }
                onActiveFocusChanged: {
                    if (!activeFocus) {
                        page.applyDescription()
                    }
                }
            }

            // Chat content tiles, each opens its list page.
            MediaKinds {
                objectName: "mediaKinds"
                // `=== true`: dconf gives `undefined` until key read.
                appsAvailable: Settings.webxdcEnabled === true
                mapAvailable: Settings.locationStreamingEnabled === true
                onKindRequested: page.openMedia(kind)
            }

            DisappearingMessages {
                objectName: "disappearing"
                seconds: chat.ephemeral_timer
                canChange: chat.can_send
                onChosen: chat.set_ephemeral_timer(seconds)
            }

            SectionHeader {
                objectName: "membersHeader"
                //: Heading over the member list. %n is how many there are.
                text: qsTr("%n member(s)", "", chat.member_count)
            }

            // Rows built on first page-in: page attaches once chat loads, so rows were
            // built for readers who never swipe over. Kept once built.
            Loader {
                id: memberSection
                objectName: "memberSection"
                width: column.width
                active: false

                sourceComponent: Column {
                    width: column.width
                    // Row gap, as in page column.
                    spacing: column.spacing

                    // Repeater, not ListView: page scrolls as one; group small.
                    Repeater {
                        model: chat.members

                        delegate: ListItem {
                            id: memberRow
                            // Named per member for tests.
                            objectName: "memberRow" + model.contact_id
                            width: column.width
                            contentHeight: body.height

                            /// Member being removed.
                            readonly property bool doomed:
                                doomedMembers.pending(model.contact_id)

                            /// Silica countdown on member; reports tap. Removal in
                            /// `doomedMembers` (rows die on change).
                            function raiseRemorse() {
                                remorse.active = true
                                //: What Silica's countdown says it is doing,
                                //: over a member the reader has asked to
                                //: remove from the group.
                                remorse.item.execute(
                                    body, qsTr("Removing"), function() {},
                                    doomedMembers.countdownFor(model.contact_id))
                            }

                            // Built on first removal, not with row.
                            Loader {
                                id: remorse
                                active: false
                                sourceComponent: RemorseItem {
                                    objectName: "memberRemorse"
                                    onCanceled: doomedMembers.spare(model.contact_id)
                                }
                            }

                            // Row rebuilt mid-wait comes back bare.
                            Component.onCompleted: {
                                if (memberRow.doomed) {
                                    memberRow.raiseRemorse()
                                }
                            }

                            // Self-removal is leaving: own pulley entry and dialog.
                            // Component built on first open (see ConversationList).
                            menu: Component {
                                ContextMenu {
                                    MenuItem {
                                        objectName: "removeItem"
                                        visible: chat.can_edit && !model.is_self
                                        text: qsTr("Remove from group")
                                        // Page told, not row: list change kills row's wait.
                                        onClicked: {
                                            doomedMembers.ask(model.contact_id)
                                            memberRow.raiseRemorse()
                                        }
                                    }
                                }
                            }

                            ContactRow {
                                id: body
                                // Remorse fades via own `opacity: 0.0`.
                                enabled: !memberRow.doomed
                                width: parent.width
                                displayName: model.display_name
                                ownColor: model.color
                                picturePath: model.avatar_path
                                isKeyContact: model.is_key_contact
                            }

                        }
                    }
                }
            }

            // Way to more members.
            PlusRow {
                objectName: "addMembersButton"
                visible: chat.can_edit
                width: column.width
                text: qsTr("Add members")
                onClicked: page.addMembers()
            }
        }
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }

    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 2
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        onDismissed: notice.text = ""
    }
}
