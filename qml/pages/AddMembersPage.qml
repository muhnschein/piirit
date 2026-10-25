import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Pick members; already-in or core-refused greyed. Adds on opener's group
 * (its own reload shows change); NewGroupPage passes a stand-in.
 */
Page {
    id: page

    property int accountId
    /// Group: ChatInfo or NewGroupPage stand-in (`is_member`, `add_members`).
    property var chat
    property string errorMessage: ""
    // Ticked contact ids.
    property var members: []

    // Empty before core answers: unknown, not none. See NewChatPage.
    property bool contactsLoaded: false

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: page.errorMessage = message
        // Emitted after rows set.
        onRows_changed: page.contactsLoaded = true
    }

    // Debounce: one search per pause.
    Timer {
        id: searchDebounce
        objectName: "searchDebounce"
        interval: 250
        onTriggered: contacts.query = searchField.text.trim()
    }

    function toggle(contactId) {
        var next = []
        var found = false
        for (var i = 0; i < page.members.length; i++) {
            if (page.members[i] === contactId) {
                found = true
            } else {
                next.push(page.members[i])
            }
        }
        if (!found) {
            next.push(contactId)
        }
        page.members = next
    }

    function isPicked(contactId) {
        for (var i = 0; i < page.members.length; i++) {
            if (page.members[i] === contactId) {
                return true
            }
        }
        return false
    }

    // Core answers on group's signals; owner shows them.
    function addPicked() {
        page.chat.add_members(page.members)
        pageStack.pop()
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

    // Page-wide flickable owns pulley; see NewGroupPage.
    SilicaFlickable {
        id: pulleyHost
        anchors.fill: parent
        contentWidth: width
        contentHeight: height

        PullDownMenu {
            MenuItem {
                objectName: "addButton"
                text: qsTr("Add to group")
                enabled: page.members.length > 0
                onClicked: page.addPicked()
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
                title: qsTr("Add members")
            }

            // Outside list: header moves with rows (see NewChatPage).
            SearchField {
                id: searchField
                objectName: "searchField"
                width: parent.width
                placeholderText: qsTr("Search")
                onTextChanged: searchDebounce.restart()
            }

            Banner {
                objectName: "errorBanner"
                width: parent.width
                text: page.errorMessage
                onDismissed: page.errorMessage = ""
            }
        }

        SilicaListView {
            id: listView
            anchors {
                top: heading.bottom
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            model: contacts.rows

            delegate: ListItem {
                objectName: "memberRow" + model.contact_id
                contentHeight: body.height
                // Already in, or address contact refused in encrypted group.
                readonly property bool addable:
                    model.is_key_contact && !page.chat.is_member(model.contact_id)
                enabled: addable

                ContactRow {
                    id: body
                    width: parent.width
                    displayName: model.display_name
                    ownColor: model.color
                    picturePath: model.avatar_path
                    isKeyContact: model.is_key_contact
                    match: contacts.query
                    opacity: addable ? 1.0 : 0.4
                }

                Label {
                    objectName: "memberMark"
                    anchors {
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: body.verticalCenter
                    }
                    visible: page.isPicked(model.contact_id)
                    text: "✓"
                    color: Theme.highlightColor
                    font.pixelSize: Theme.fontSizeLarge
                }

                onClicked: if (addable) page.toggle(model.contact_id)
            }

            ViewPlaceholder {
                objectName: "contactsPlaceholder"
                // Not until core answered: see `contactsLoaded`.
                enabled: page.contactsLoaded && contacts.count === 0
                text: searchField.text.trim().length > 0 ? qsTr("Nobody matches")
                                                          : qsTr("No contacts to add")
            }
        }
    }
}
