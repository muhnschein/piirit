import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Every member of a group, from its page's Members row. Add members and Invite in
 * pulley, search on top, members built as they scroll in (groups reach hundreds).
 * Tap opens a member, hold for Send message and Remove.
 */
Page {
    id: page

    property int accountId
    /// Group's ChatInfo, shared with its page: search filters its `members`.
    property QtObject chat

    readonly property bool canAdd: page.chat.can_edit && !page.chat.is_channel
    readonly property bool canInvite: page.canAdd || page.chat.is_own_channel

    /// Removal wait, ms. Tests lower it.
    property alias pendingDelay: removals.delay

    /// Members asked to remove, in undo window. See PendingRemoval.
    PendingRemoval {
        id: removals
        onRemove: page.chat.remove_member(id)
    }

    // Silica's countdown over a row; removal is `removals`' (outlives row): no-op callback.
    Component {
        id: remorseMaker
        RemorseItem {
            objectName: "memberRemorse"
            property int contactId
            onCanceled: removals.spare(contactId)
        }
    }

    function raiseRemorse(row) {
        if (!row.remorse) {
            row.remorse = remorseMaker.createObject(row, { contactId: row.contactId })
        }
        //: What Silica's countdown says it is doing, over a member the
        //: reader has asked to remove from the group.
        row.remorse.execute(row.body, qsTr("Removing"), function() {},
                            removals.countdownFor(row.contactId))
    }

    MemberOpener {
        id: memberOpener
        chatInfo: page.chat
        accountId: page.accountId
    }

    // Removals go on leave.
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            removals.flush()
        }
    }

    // Search filters the shared model: cleared once gone.
    Component.onDestruction: {
        if (page.chat) {
            page.chat.member_filter = ""
        }
    }

    // Search field outside list: header field would move with keystrokes. Non-scrolling
    // flickable owns pulley, as ChatListPage.
    SilicaFlickable {
        anchors.fill: parent
        contentWidth: width
        contentHeight: height

        // The two ways a group grows. Channel: core takes no members added. Menu's `visible`
        // not from items': theirs reads menu's, never turns true.
        PullDownMenu {
            objectName: "membersMenu"
            visible: page.canInvite

            MenuItem {
                id: addMembersItem
                objectName: "addMembersItem"
                visible: page.canAdd
                text: qsTr("Add members")
                onClicked: pageStack.push(Qt.resolvedUrl("AddMembersPage.qml"), {
                    accountId: page.accountId,
                    chat: page.chat
                })
            }
            MenuItem {
                id: inviteItem
                objectName: "inviteItem"
                visible: page.canInvite
                //: Pulley on a group's member list: opens the QR code others scan to join.
                text: qsTr("Invite with a QR code")
                onClicked: pageStack.push(Qt.resolvedUrl("QrPage.qml"), {
                    accountId: page.accountId,
                    chatId: page.chat.chat_id,
                    chatName: page.chat.name,
                    isChannel: page.chat.is_channel
                })
            }
        }

        Column {
            id: heading
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }

            PageHeader {
                objectName: "membersHeader"
                //: Heading of a group's member list. %n is how many there are.
                title: qsTr("%n member(s)", "", page.chat ? page.chat.member_count : 0)
                description: page.chat ? page.chat.name : ""
            }

            SearchField {
                id: searchField
                objectName: "searchField"
                width: parent.width
                //: Prompt in the search field over a group's member list.
                placeholderText: qsTr("Search members")
                onTextChanged: page.chat.member_filter = searchField.text
            }
        }

        SilicaListView {
            id: list
            objectName: "memberList"
            anchors {
                top: heading.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            // Else rows draw outside box: see NewChatPage.
            clip: true
            model: page.chat ? page.chat.members : null

            delegate: MemberRow {
                id: memberRow
                objectName: "memberRow" + model.contact_id
                width: list.width
                contactId: model.contact_id
                displayName: model.display_name
                ownColor: model.color
                picturePath: model.avatar_path
                isKeyContact: model.is_key_contact
                isSelf: model.is_self
                match: page.chat.member_filter
                canRemove: page.chat.can_edit
                doomed: removals.pending(model.contact_id)
                opener: memberOpener
                onRemoveAsked: {
                    removals.ask(model.contact_id)
                    page.raiseRemorse(memberRow)
                }
                // Row rebuilt mid-wait comes back bare.
                Component.onCompleted: {
                    if (memberRow.doomed) {
                        page.raiseRemorse(memberRow)
                    }
                }
            }

            ViewPlaceholder {
                objectName: "membersPlaceholder"
                enabled: page.chat.loaded && searchField.text.trim().length > 0
                         && list.count === 0
                text: qsTr("Nobody matches")
            }

            VerticalScrollDecorator {}
        }
    }
}
