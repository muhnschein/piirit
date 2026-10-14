import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media
import Piirit 1.0

/*
 * A group after it has been made: its picture, its name, and who is in it.
 * Reached by swiping left from the conversation, or tapping its header.
 *
 * All of it is the core's, so this page owns a ChatInfo over
 * get_full_chat_by_id rather than a record of its own, and every change
 * goes to the core the moment it is made -- the name a pause after typing
 * stops and again on the way out, as the profile page does, and
 * everything else on the tap. Nothing here needs saving.
 *
 * What can be changed is the core's call too: a group this account has
 * left, or a chat that is not a group, refuses every edit, so the
 * controls for them are not offered.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// The name the conversation page showed, until the core answers.
    property string chatName

    /// The group was given a new name here.
    signal renamed(string name)

    ChatInfo {
        id: chat
        objectName: "chat"
        account_id: page.accountId
        chat_id: page.chatId
        onError: {
            page.errorMessage = message
            // What was typed did not reach the core, so the field goes
            // back to what the core holds rather than showing a
            // description nobody else has.
            page.refillDescription()
        }
        // Filled from the core, never re-filled from it while someone is
        // typing: every change reloads, and that would reach in and reset
        // the cursor.
        onLoaded_changed: {
            if (chat.loaded && !page.edited) {
                // Assigning to the field fires onTextChanged, which is the
                // same signal the reader typing produces.
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
        // A group that has been left is still a chat, with its history in
        // it, so the way out is back to it rather than to the list. The
        // confirmation's own pop may still be running when the core
        // answers, and Silica drops a stack operation asked for during a
        // transition -- so that one is finished first.
        onLeft: {
            if (pageStack.completeAnimation) {
                pageStack.completeAnimation()
            }
            pageStack.pop()
        }
    }

    Connections {
        target: core
        // Renaming and joining reach here from other devices too; the
        // model ignores what is not this chat's.
        onCore_event: chat.handle_event(context_id, kind, payload_json)
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                chat.reload()
            }
        }
    }

    /// Someone has typed since the load. Guards the refill above.
    property bool edited: false
    /// The refill is writing to the field, so the change is not an edit.
    property bool filling: false
    /// The description field has been typed in since the last save or
    /// load. Guards its refill, as `edited` does the name's.
    property bool descriptionEdited: false
    property string errorMessage: ""

    // A pause, not a keystroke: a round trip per letter would be seven
    // calls to write "Walkers".
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
        // A group has to be called something: a blanked field goes back
        // to the name the group has, rather than to the core as nothing.
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

    // Not on a pause, as the name is: the core sends every change of the
    // description to all the members as a message of its own, so saving
    // after each pause in typing would put several into the chat. It goes
    // when the field loses the cursor, and when the page is left.
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

    // Leaving is the other moment worth saving at: a back-swipe within
    // the pause above would otherwise drop what was typed. The name goes
    // back to being a name on the way out too, and any member still
    // waiting to be removed goes now -- leaving is exactly when a timer
    // has not fired yet.
    onStatusChanged: {
        page.showMembersOnceShown()
        if (status === PageStatus.Deactivating) {
            page.applyEdits()
            page.applyDescription()
            nameField.done()
            doomedMembers.flush()
        }
    }

    /// Build the member rows the first time the page is coming onto the
    /// screen; see `memberSection`. On its way in rather than once it is
    /// there, so the rows are already in place as it slides across.
    function showMembersOnceShown() {
        if (page.status === PageStatus.Activating
                || page.status === PageStatus.Active) {
            memberSection.active = true
        }
    }

    // A page made already on the screen never sees its status change on
    // the way in.
    Component.onCompleted: page.showMembersOnceShown()

    /// How long a member waits before they go, in milliseconds. Nothing
    /// sets it; a test turns it down rather than waiting.
    property alias pendingDelay: doomedMembers.delay

    /// The members the reader has asked to remove, waiting out the
    /// moment in which they can say they did not mean it.
    ///
    /// Not Silica's `remorseAction`, which would put the wait on the row:
    /// removing one member reloads the member list, so the row of the
    /// second one asked for -- and the wait on it -- was destroyed before
    /// it ever ran. See PendingRemoval.
    PendingRemoval {
        id: doomedMembers
        onRemove: chat.remove_member(id)
    }

    // The group's picture, as big as the screen will show it, on the page
    // a picture in the chat opens on. Nothing to show for a group that has
    // none: the disc with its initial is not a picture.
    function showPicture() {
        if (chat.avatar_path.length === 0) {
            return
        }
        pageStack.push(Qt.resolvedUrl("PicturePage.qml"), {
            fileUrl: Media.fileUrl(chat.avatar_path),
            fileName: ""
        })
    }

    // The gallery, pushed by URL and connected to, the way the settings
    // page picks a profile picture: the Attach*Page files are the only
    // ones that name a `Sailfish.Pickers` type, so a type that is not
    // there costs this button rather than the page.
    function pickPicture() {
        var picker = pageStack.push(Qt.resolvedUrl("AttachPhotoPage.qml"))
        if (picker) {
            picker.picked.connect(function(path) {
                // The core copies the file into its own blob directory,
                // so the picked one may go away afterwards.
                chat.set_picture(path)
            })
        }
    }

    // Every kind of thing the chat holds, on a page of its own: pushed
    // by name, as every page is, and the tile says which kind.
    function openMedia(kind) {
        pageStack.push(Qt.resolvedUrl("ChatMediaPage.qml"), {
            accountId: page.accountId,
            chatId: page.chatId,
            kind: kind
        })
    }

    function addMembers() {
        pageStack.push(Qt.resolvedUrl("AddMembersPage.qml"), {
            accountId: page.accountId,
            chat: chat
        })
    }

    // Asked on a page of its own, with the platform's accept at the top,
    // rather than counted down: there is no way back in without someone
    // else adding you, and a countdown on a pull-down is easy to miss.
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

        // Both entries belong to a member, so for a reader who has left
        // the group -- or was removed from it -- the menu would be empty,
        // and an empty pull-down is a pull that does nothing.
        PullDownMenu {
            objectName: "groupMenu"
            visible: chat.can_edit

            MenuItem {
                objectName: "leaveButton"
                visible: chat.can_edit
                text: qsTr("Leave group")
                onClicked: page.confirmLeave()
            }
            // Only offered when there is one to remove.
            MenuItem {
                objectName: "removePicture"
                visible: chat.can_edit && chat.avatar_path.length > 0
                text: qsTr("Remove picture")
                onClicked: chat.clear_picture()
            }
            // Here rather than on the picture, which a tap shows larger,
            // as it does on a contact's page.
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
                title: qsTr("Group")
            }

            // A tap shows the picture larger; changing it is in the
            // pull-down.
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

            // The name, under the picture: a field, on a group this
            // account is still in. See EditableName.qml.
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

            // What the group says about itself. Shown to anyone, and a
            // field only for those the core lets change it; it keeps the
            // line breaks it was written with. Nothing is drawn for a
            // group that has none and cannot be given one.
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

            // What the group holds besides words, a tile per kind, each a
            // way into the page that lists it. The row keeps its own room
            // above and below, between what the group is and what it does.
            MediaKinds {
                objectName: "mediaKinds"
                // `=== true` because dconf hands back `undefined` before it
                // has read the key.
                appsAvailable: Settings.webxdcEnabled === true
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

            // The rows themselves, built the first time the page is on
            // its way onto the screen rather than when it is made. A
            // group's page is made attached to its conversation as soon
            // as the conversation has loaded, so every member's row was
            // built while the reader was only opening the chat -- most
            // of the time to never swipe across to it. Kept once built:
            // the next swipe across should not build them again.
            Loader {
                id: memberSection
                objectName: "memberSection"
                width: column.width
                active: false

                sourceComponent: Column {
                    width: column.width
                    // The page's own gap between rows, which the rows had
                    // when they stood in the page's column themselves.
                    spacing: column.spacing

                    // A Repeater in the column rather than a list of its
                    // own: the page scrolls as one thing, and a group is
                    // short enough.
                    Repeater {
                        model: chat.members

                        delegate: ListItem {
                            id: memberRow
                            // Named per member, so a test can find the one
                            // it means rather than the first.
                            objectName: "memberRow" + model.contact_id
                            width: column.width
                            contentHeight: body.height

                            /// This member is on their way out.
                            readonly property bool doomed:
                                doomedMembers.pending(model.contact_id)

                            /// Silica's own countdown, drawn over the
                            /// member. The removal is not its business --
                            /// that belongs to `doomedMembers`, because a
                            /// remorse item lives in the row it covers and
                            /// a member's row can go whenever the list
                            /// changes. So it draws, and reports the tap.
                            function raiseRemorse() {
                                remorse.active = true
                                //: What Silica's countdown says it is doing,
                                //: over a member the reader has asked to
                                //: remove from the group.
                                remorse.item.execute(
                                    body, qsTr("Removing"), function() {},
                                    doomedMembers.countdownFor(model.contact_id))
                            }

                            // Built the first time a removal is asked for,
                            // not with the row, as the conversation does.
                            Loader {
                                id: remorse
                                active: false
                                sourceComponent: RemorseItem {
                                    objectName: "memberRemorse"
                                    onCanceled: doomedMembers.spare(model.contact_id)
                                }
                            }

                            // A row rebuilt mid-wait comes back bare.
                            Component.onCompleted: {
                                if (memberRow.doomed) {
                                    memberRow.raiseRemorse()
                                }
                            }

                            // Removing yourself is leaving, which has its
                            // own place in the pulley and its own
                            // countdown. A Component, which Silica builds
                            // the first time the menu is opened; see
                            // ConversationList.
                            menu: Component {
                                ContextMenu {
                                    MenuItem {
                                        objectName: "removeItem"
                                        visible: chat.can_edit && !model.is_self
                                        text: qsTr("Remove from group")
                                        // The page is told, not this row:
                                        // removing one member changes the
                                        // list, and a wait living on a row
                                        // would go with the row.
                                        onClicked: {
                                            doomedMembers.ask(model.contact_id)
                                            memberRow.raiseRemorse()
                                        }
                                    }
                                }
                            }

                            ContactRow {
                                id: body
                                // The remorse covering this does the
                                // fading, with its own `opacity: 0.0` on
                                // what it was handed.
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

            // The way to more members.
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
