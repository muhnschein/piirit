import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

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
        onError: page.errorMessage = message
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
        if (status === PageStatus.Deactivating) {
            page.applyEdits()
            nameField.done()
            doomedMembers.flush()
        }
    }

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
            // Only offered when there is one to remove, and kept off the
            // picture itself: a tap that might delete is not a tap you
            // want under a finger reaching for the gallery.
            MenuItem {
                objectName: "removePicture"
                visible: chat.can_edit && chat.avatar_path.length > 0
                text: qsTr("Remove picture")
                onClicked: chat.clear_picture()
            }
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Group")
            }

            // The picture is the control, not a preview of one: tapping
            // it opens the gallery, and the badge says so.
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

                Rectangle {
                    id: editBadge
                    objectName: "editBadge"
                    visible: chat.can_edit
                    anchors {
                        right: bigAvatar.right
                        bottom: bigAvatar.bottom
                    }
                    width: Theme.itemSizeExtraSmall
                    height: width
                    radius: width / 2
                    color: Theme.highlightBackgroundColor

                    Image {
                        anchors.centerIn: parent
                        source: "image://theme/icon-s-edit"
                    }
                }

                MouseArea {
                    objectName: "pictureTap"
                    anchors.fill: bigAvatar
                    enabled: chat.can_edit
                    onClicked: page.pickPicture()
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

            // A Repeater in the column rather than a list of its own: the
            // page scrolls as one thing, and a group is short enough.
            Repeater {
                model: chat.members

                delegate: ListItem {
                    id: memberRow
                    // Named per member, so a test can find the one it
                    // means rather than the first.
                    objectName: "memberRow" + model.contact_id
                    width: column.width
                    contentHeight: body.height

                    /// This member is on their way out.
                    readonly property bool doomed:
                        doomedMembers.pending(model.contact_id)

                    /// Silica's own countdown, drawn over the member.
                    /// The removal is not its business -- that belongs
                    /// to `doomedMembers`, because a remorse item lives
                    /// in the row it covers and removing one member
                    /// reloads the whole member list. So it draws, and
                    /// reports the tap.
                    function raiseRemorse() {
                        //: What Silica's countdown says it is doing,
                        //: over a member the reader has asked to
                        //: remove from the group.
                        remorse.execute(
                            body, qsTr("Removing"), function() {},
                            doomedMembers.countdownFor(model.contact_id))
                    }

                    RemorseItem {
                        id: remorse
                        objectName: "memberRemorse"
                        onCanceled: doomedMembers.spare(model.contact_id)
                    }

                    // A row rebuilt mid-wait comes back bare.
                    Component.onCompleted: {
                        if (memberRow.doomed) {
                            memberRow.raiseRemorse()
                        }
                    }

                    // Removing yourself is leaving, which has its own
                    // place in the pulley and its own countdown.
                    menu: ContextMenu {
                        MenuItem {
                            objectName: "removeItem"
                            visible: chat.can_edit && !model.is_self
                            text: qsTr("Remove from group")
                            // The page is told, not this row: removing
                            // one member reloads the list, and a wait
                            // living on a row would go with the row.
                            onClicked: {
                                doomedMembers.ask(model.contact_id)
                                memberRow.raiseRemorse()
                            }
                        }
                    }

                    ContactRow {
                        id: body
                        // The remorse covering this does the fading,
                        // with its own `opacity: 0.0` on what it was
                        // handed.
                        enabled: !memberRow.doomed
                        width: parent.width
                        displayName: model.display_name
                        ownColor: model.color
                        picturePath: model.avatar_path
                        isKeyContact: model.is_key_contact
                        isVerified: model.is_verified
                    }

                }
            }

            // The way to more members, where the next one would be listed:
            // a row shaped like a member's, with a plus for a picture.
            ListItem {
                id: addMembersRow
                objectName: "addMembersButton"
                visible: chat.can_edit
                width: column.width
                contentHeight: Theme.itemSizeSmall + 2 * Theme.paddingMedium

                PlusMark {
                    id: plus
                    x: Theme.horizontalPageMargin
                    y: Theme.paddingMedium
                }

                Label {
                    x: plus.x + plus.width + Theme.paddingMedium
                    width: parent.width - x - Theme.horizontalPageMargin
                    anchors.verticalCenter: plus.verticalCenter
                    wrapMode: Text.Wrap
                    color: addMembersRow.highlighted ? Theme.highlightColor
                                                     : Theme.primaryColor
                    text: qsTr("Add members")
                }

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
