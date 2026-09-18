import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Name a group, give it a picture and pick its members, laid out the way
 * the group's own page will be once it exists (GroupPage.qml): the
 * picture with its badge, the name under it, the members, and the way to
 * more of them where the next one would be listed. What is different is
 * what is not there yet -- the group -- so everything chosen here is held
 * on the page and handed to the core in one go when the group is made.
 * Groups are created encrypted, which is what the reference client's
 * "New Group" does.
 *
 * The members are asked for by id (`ContactList.picked_rows`) rather than
 * drawn by hiding everybody else. A Repeater over every contact built a
 * row -- and an avatar, and the two effects behind it -- for each one,
 * twice over, the moment the contacts landed, which is the moment this
 * page was arriving over the chat list; what it cost to open was the
 * size of an address book that had nothing to do with the group.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""
    // True from tapping create until the core answers, so a second tap
    // cannot make a second group.
    property bool creating: false
    /// Contact ids picked so far, besides the reader's own.
    property var members: []

    /// The reader's own row, as a list of one -- or of none, until the
    /// contacts have arrived. A list because that is what a Repeater
    /// takes, and the row is drawn apart from the picked ones: there is
    /// nothing to do with oneself here, so it carries no menu.
    property var selfRow: []
    /// The picked members' rows, in the order they were picked.
    property var pickedRows: []

    /// Ask the contact list for the rows to draw, and split off the
    /// reader's own. Run whenever either half can have changed: the
    /// picks, and the contacts they are looked up in.
    function refreshMemberRows() {
        var rows = contacts.picked_rows(page.members)
        var mine = rows.length > 0 && rows[0].is_self ? [rows[0]] : []
        page.selfRow = mine
        page.pickedRows = rows.slice(mine.length)
    }

    /// The picture chosen for the group, empty for none. Held as a path:
    /// the core takes a picture only for a chat that exists.
    property string picturePath: ""

    /// What AddMembersPage asks of the group it adds to. There is no
    /// group yet, so the page answers for it: who is in, and whom to add.
    readonly property var pendingGroup: ({
        is_member: function(contactId) { return page.isMember(contactId) },
        add_members: function(contactIds) { page.addMembers(contactIds) }
    })

    // Every contact, the reader's own last: the members are drawn from
    // these rows, so a picked id has a name and a picture to show.
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
            pageStack.replace(Qt.resolvedUrl("ConversationPage.qml"), {
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

    /// Take a picked member off again. Nothing has been sent, so there
    /// is nothing to undo on the core.
    function removeMember(contactId) {
        var next = []
        for (var i = 0; i < page.members.length; i++) {
            if (page.members[i] !== contactId) {
                next.push(page.members[i])
            }
        }
        page.members = next
    }

    /// Add what the picker ticked, once each.
    function addMembers(contactIds) {
        var next = page.members.slice()
        for (var i = 0; i < contactIds.length; i++) {
            if (!page.isMember(contactIds[i])) {
                next.push(contactIds[i])
            }
        }
        page.members = next
    }

    // The same picker the group's page uses, handed the stand-in above
    // where that page hands its ChatInfo.
    function pickMembers() {
        pageStack.push(Qt.resolvedUrl("AddMembersPage.qml"), {
            accountId: page.accountId,
            chat: page.pendingGroup
        })
    }

    // The gallery, through the same page GroupPage uses, so the picker
    // page is the one place Sailfish.Pickers is imported.
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
            page.errorMessage = qsTr("Please name the group")
            return
        }
        page.errorMessage = ""
        page.creating = true
        nameField.done()
        contacts.create_group(name, page.members, page.picturePath)
    }

    // Picking somebody, or taking them off again, changes what is drawn.
    onMembersChanged: page.refreshMemberRows()

    // The name is what a new group needs first, so the cursor is in the
    // field when the page arrives.
    onStatusChanged: {
        if (status === PageStatus.Active && nameField.text.length === 0) {
            nameField.edit()
        }
    }

    Connections {
        target: core
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                contacts.reload()
            }
        }
    }

    Connections {
        target: contacts
        // The contacts arrive after the page does, and the reader's own
        // row is among them.
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
                text: qsTr("Create Group")
                enabled: !page.creating && nameField.text.trim().length > 0
                onClicked: page.createGroup()
            }
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("New group")
            }

            // The picture is the control, not a preview of one: tapping
            // it opens the gallery, and the badge says so. Until one is
            // chosen it shows the name's initial, as the group will.
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

            // The name, under the picture, as the group's page shows it.
            // The field is what is read; see EditableName.qml.
            EditableName {
                id: nameField
                objectName: "groupNameControl"
                fieldObjectName: "nameField"
                hintObjectName: "nameHint"
                placeholderText: qsTr("Group name")
                hint: qsTr("Everyone in the group sees the name")
            }

            Banner {
                objectName: "errorBanner"
                width: parent.width
                text: page.errorMessage
                onDismissed: page.errorMessage = ""
            }

            SectionHeader {
                objectName: "membersHeader"
                //: Heading over the member list. %n is how many there are, the reader included.
                text: qsTr("%n member(s)", "", page.members.length + 1)
            }

            // The reader, who is in the group by being the one making
            // it. No menu: leaving a group one is making is not making
            // it.
            Repeater {
                model: page.selfRow

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
                        isVerified: modelData.is_verified
                    }
                }
            }

            // Then whoever was picked, in the order they were picked.
            Repeater {
                model: page.pickedRows

                delegate: ListItem {
                    objectName: "memberRow" + modelData.contact_id
                    width: column.width
                    contentHeight: picked.height

                    // Taking someone off the list again, from the row,
                    // as the group's page offers it. No countdown: nothing
                    // has been sent yet.
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
                        isVerified: modelData.is_verified
                    }
                }
            }

            // The way to more members, where the next one would be listed:
            // a row shaped like a member's, with a plus for a picture.
            ListItem {
                id: addMembersRow
                objectName: "addMembersButton"
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

                onClicked: page.pickMembers()
            }
        }
    }
}
