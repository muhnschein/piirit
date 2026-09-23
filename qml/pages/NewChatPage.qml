import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Pick someone to talk to: the known contacts, and a search over them.
 *
 * Nothing here adds a contact. An address alone produces a chat that
 * cannot be encrypted, and an invite is how a Delta Chat contact is
 * actually added -- the other person's code held up to the camera, or
 * their link typed -- which is the QR code page, in the chat list's
 * pull-down beside this one. A group is made from there too.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    // The core has said who is in this profile, whatever that was.
    //
    // An empty model means one of two things and they are opposite: no
    // contacts, or no answer yet. For the first moment after the page
    // opens the second is what is true, and "No contacts yet" over a
    // list about to fill is the app telling the reader something it
    // does not know. It flashed under the rows on every visit.
    property bool contactsLoaded: false

    // Not bound straight to the field: a round trip per keystroke asks the
    // core four times to type "anna", and only the last answer is wanted.
    Timer {
        id: searchDebounce
        interval: 250
        onTriggered: contacts.query = searchField.text
    }

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: page.errorMessage = message
        // Emitted once the rows have been set, whether there turned out
        // to be any or none.
        onRows_changed: page.contactsLoaded = true
        // Open the chat that now exists, above the page that opened this
        // one: this page has done its job.
        onChat_ready: pageStack.replaceAbove(pageStack.previousPage(page),
                                             Qt.resolvedUrl("ConversationPage.qml"), {
            accountId: page.accountId,
            chatId: chat_id,
            chatName: qsTr("Chat")
        })
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

    // The search field outside the list, for the reason ChatListPage
    // documents: a field in a view's `header` lives inside the flickable
    // and moves on every keystroke, which is what took the keyboard away
    // mid-search. A flickable that does not scroll holds the two.
    SilicaFlickable {
        id: host
        anchors.fill: parent
        contentWidth: width
        contentHeight: height

        Column {
            id: heading
            anchors {
                top: parent.top
                left: parent.left
                right: parent.right
            }

            PageHeader {
                title: qsTr("New chat")
            }

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
            // Rows draw outside the list's own box otherwise, and the
            // box starts under the search field: without this a flick
            // ran the contacts up over the field.
            clip: true
            model: contacts.rows

            // No context menu: picking a contact is the only thing to do
            // with one here.
            delegate: ListItem {
                objectName: "contactRow"
                contentHeight: body.height

                ContactRow {
                    id: body
                    width: parent.width
                    displayName: model.display_name
                    ownColor: model.color
                    picturePath: model.avatar_path
                    isKeyContact: model.is_key_contact
                }

                onClicked: contacts.open_chat_with(model.contact_id)
            }

            ViewPlaceholder {
                objectName: "contactsPlaceholder"
                // Not until the core has answered: see `contactsLoaded`.
                enabled: page.contactsLoaded && contacts.count === 0
                text: qsTr("No contacts yet")
                hintText: qsTr("Use \"QR code\" in the chat list to add a contact.")
            }
        }
    }
}
