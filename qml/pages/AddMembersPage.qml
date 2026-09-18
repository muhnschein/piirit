import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Pick who to add to a group: everyone already in is greyed, and so is
 * anyone the core would refuse. A search field at the top narrows the
 * list, for the reader with more contacts than fit on a screen.
 *
 * The group is the page that opened this one's, handed in rather than
 * loaded again, and the adding is done on it -- so what it shows is
 * updated by the same reload that answers every other change to it.
 * For a group that does not exist yet, NewGroupPage hands in a stand-in
 * that answers the same two questions.
 */
Page {
    id: page

    property int accountId
    /// The group being added to: a ChatInfo, or NewGroupPage's stand-in
    /// for one. Asked `is_member(contactId)` and told `add_members(ids)`.
    property var chat
    property string errorMessage: ""
    // Contact ids the user has ticked.
    property var members: []

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: page.errorMessage = message
    }

    // A keystroke's worth of quiet before the core is asked, so typing
    // a name is one search rather than one per letter.
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

    // Straight back to the group: the core answers on the group's own
    // signals, and the page that owns it is the one showing them.
    function addPicked() {
        page.chat.add_members(page.members)
        pageStack.pop()
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

    // One flickable for the whole page, owning the pulley; see
    // NewGroupPage.
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

            // Outside the list, for the reason NewChatPage documents: a
            // field in a view's header moves with the rows under it.
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
                // Greyed where there is nothing to do: already in, or an
                // address contact the core will not take into an
                // encrypted group.
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
                    isVerified: model.is_verified
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
                enabled: contacts.count === 0
                text: searchField.text.trim().length > 0 ? qsTr("Nobody matches")
                                                          : qsTr("No contacts to add")
            }
        }
    }
}
