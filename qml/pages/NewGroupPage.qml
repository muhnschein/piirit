import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Name group, picture, members; laid out as GroupPage.qml. Sent to core in
 * one go at create, encrypted ("New Group"). Members by id
 * (`ContactList.picked_rows`): Repeater per contact scaled with address book.
 *
 * `channel`: own channel instead ("New Channel"), name and picture only. Core
 * takes no members for one: subscribers join by its invite (GroupPage.qml).
 */
Page {
    id: page

    // Core move held until on screen: else may take covering call page
    // (PendingNavigation.qml).
    PendingNavigation {
        id: navigation
        stack: pageStack
        ready: page.status === PageStatus.Active
    }

    property int accountId
    /// Make channel, not group.
    property bool channel: false
    /// Page chat opens over; null = page under this one (NewDialog.qml passes chat list).
    property var landOn: null
    property string errorMessage: ""
    // Create tap until core answers: no second group from second tap.
    property bool creating: false
    /// Picked contact ids, besides own.
    property var members: []

    /// Own row as list of one, none until contacts load. No menu on self.
    property var selfRow: []
    /// Picked members' rows, in pick order.
    property var pickedRows: []

    /// Ask contact list for rows; split off own. Rerun on any change.
    function refreshMemberRows() {
        var rows = contacts.picked_rows(page.members)
        var mine = rows.length > 0 && rows[0].is_self ? [rows[0]] : []
        page.selfRow = mine
        page.pickedRows = rows.slice(mine.length)
    }

    /// Picture path; empty for none. Core takes it only for existing chat.
    property string picturePath: ""

    /// What AddMembersPage asks: no group yet, so page answers.
    readonly property var pendingGroup: ({
        is_member: function(contactId) { return page.isMember(contactId) },
        add_members: function(contactIds) { page.addMembers(contactIds) }
    })

    // All contacts, own last: picked ids need name and picture.
    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        include_self: true
        onError: {
            page.creating = false
            page.errorMessage = message
        }
        onChat_ready: {
            page.creating = false
            // Over chat list, not over dialog: back from chat skips both.
            navigation.replaceAbove(page.landOn || pageStack.previousPage(page),
                                    Qt.resolvedUrl("ConversationPage.qml"), {
                accountId: page.accountId,
                chatId: chat_id,
                chatName: nameField.text.trim()
            })
        }
    }

    function isMember(contactId) {
        for (var i = 0; i < page.members.length; i++) {
            if (page.members[i] === contactId) {
                return true
            }
        }
        return false
    }

    /// Unpick member. Nothing sent, nothing to undo.
    function removeMember(contactId) {
        var next = []
        for (var i = 0; i < page.members.length; i++) {
            if (page.members[i] !== contactId) {
                next.push(page.members[i])
            }
        }
        page.members = next
    }

    /// Add ticked picks, once each.
    function addMembers(contactIds) {
        var next = page.members.slice()
        for (var i = 0; i < contactIds.length; i++) {
            if (!page.isMember(contactIds[i])) {
                next.push(contactIds[i])
            }
        }
        page.members = next
    }

    // Same picker as group page; stand-in given where it gives ChatInfo.
    function pickMembers() {
        pageStack.push(Qt.resolvedUrl("AddMembersPage.qml"), {
            accountId: page.accountId,
            chat: page.pendingGroup
        })
    }

    // Gallery via AttachPhotoPage, as GroupPage: only picker pages import
    // Sailfish.Pickers.
    function pickPicture() {
        var picker = pageStack.push(Qt.resolvedUrl("AttachPhotoPage.qml"))
        if (picker) {
            picker.picked.connect(function(path) {
                page.picturePath = path
            })
        }
    }

    function createGroup() {
        var name = nameField.text.trim()
        if (name.length === 0) {
            page.errorMessage = page.channel ? qsTr("Please name the channel")
                                             : qsTr("Please name the group")
            return
        }
        page.errorMessage = ""
        page.creating = true
        nameField.done()
        if (page.channel) {
            contacts.create_channel(name, page.picturePath)
        } else {
            contacts.create_group(name, page.members, page.picturePath)
        }
    }

    // Pick or unpick changes what is drawn.
    onMembersChanged: page.refreshMemberRows()

    // Name first: cursor in field on arrival.
    onStatusChanged: {
        if (status === PageStatus.Active && nameField.text.length === 0) {
            nameField.edit()
        }
    }

    Connections {
        target: core
        // Model made before core up: nothing to load.
        onStatus_changed: {
            if (core.status === "ready") {
                contacts.reload()
            }
        }
    }

    Connections {
        target: contacts
        // Contacts arrive after page; own row among them.
        onRows_changed: page.refreshMemberRows()
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            MenuItem {
                objectName: "removePicture"
                visible: page.picturePath.length > 0
                text: qsTr("Remove picture")
                onClicked: page.picturePath = ""
            }
            MenuItem {
                objectName: "createButton"
                text: page.channel ? qsTr("Create channel") : qsTr("Create group")
                enabled: !page.creating && nameField.text.trim().length > 0
                onClicked: page.createGroup()
            }
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                objectName: "pageHeader"
                //: Delta Chat's "new_channel".
                title: page.channel ? qsTr("New channel") : qsTr("New group")
            }

            // Picture is the control: tap opens gallery, badge says so. Name's initial
            // until chosen.
            Item {
                width: parent.width
                height: bigAvatar.height + 2 * Theme.paddingLarge

                Avatar {
                    id: bigAvatar
                    objectName: "groupAvatar"
                    anchors.centerIn: parent
                    width: 2 * Theme.itemSizeExtraLarge
                    initial: nameField.text
                    ownColor: ""
                    picturePath: page.picturePath
                }

                Rectangle {
                    id: editBadge
                    objectName: "editBadge"
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
                    onClicked: page.pickPicture()
                }
            }

            // Name under picture (EditableName.qml).
            EditableName {
                id: nameField
                objectName: "groupNameControl"
                fieldObjectName: "nameField"
                hintObjectName: "nameHint"
                //: Delta Chat's "channel_name".
                placeholderText: page.channel ? qsTr("Channel name") : qsTr("Group name")
                hint: page.channel ? qsTr("Everyone in the channel sees the name")
                                   : qsTr("Everyone in the group sees the name")
            }

            Banner {
                objectName: "errorBanner"
                width: parent.width
                text: page.errorMessage
                onDismissed: page.errorMessage = ""
            }

            // Members: group only.
            SectionHeader {
                objectName: "membersHeader"
                visible: !page.channel
                //: Heading over the member list. %n is how many there are, the reader included.
                text: qsTr("%n member(s)", "", page.members.length + 1)
            }

            // Reader is in group by making it; no menu.
            Repeater {
                model: page.channel ? [] : page.selfRow

                delegate: ListItem {
                    objectName: "memberRow" + modelData.contact_id
                    width: column.width
                    contentHeight: body.height
                    menu: null

                    ContactRow {
                        id: body
                        width: parent.width
                        displayName: modelData.display_name
                        ownColor: modelData.color
                        picturePath: modelData.avatar_path
                        isKeyContact: modelData.is_key_contact
                    }
                }
            }

            // Picked members, in pick order.
            Repeater {
                model: page.pickedRows

                delegate: ListItem {
                    objectName: "memberRow" + modelData.contact_id
                    width: column.width
                    contentHeight: picked.height

                    // Unpick from row; no countdown: nothing sent.
                    menu: ContextMenu {
                        MenuItem {
                            objectName: "removeItem"
                            text: qsTr("Remove from group")
                            onClicked: page.removeMember(modelData.contact_id)
                        }
                    }

                    ContactRow {
                        id: picked
                        width: parent.width
                        displayName: modelData.display_name
                        ownColor: modelData.color
                        picturePath: modelData.avatar_path
                        isKeyContact: modelData.is_key_contact
                    }
                }
            }

            // Way to more members.
            PlusRow {
                objectName: "addMembersButton"
                visible: !page.channel
                width: column.width
                text: qsTr("Add members")
                onClicked: page.pickMembers()
            }
        }
    }
}
