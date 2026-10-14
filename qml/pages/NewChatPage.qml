import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Pick contact: known contacts plus search.
 * Adds no contact: address alone gives unencryptable chat.
 * Contacts come from invites (QR page, chat list pull-down); groups too.
 */
Page {
    id: page

    // Core move held until on screen: call page may cover it.
    // See PendingNavigation.qml.
    PendingNavigation {
        id: navigation
        stack: pageStack
        ready: page.status === PageStatus.Active
    }

    property int accountId
    property string errorMessage: ""

    // Core has said who is in profile.
    //
    // Empty model: no contacts or no answer yet. Right after open, the
    // latter; "No contacts yet" flashed under rows every visit.
    property bool contactsLoaded: false

    // Not bound to field: round trip per keystroke, only last answer used.
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
        // Emitted once rows set.
        onRows_changed: page.contactsLoaded = true
        // Open new chat above this page: job done.
        onChat_ready: navigation.replaceAbove(pageStack.previousPage(page),
                                              Qt.resolvedUrl("ConversationPage.qml"), {
            accountId: page.accountId,
            chatId: chat_id,
            chatName: qsTr("Chat")
        })
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

    // Search outside list (see ChatListPage): in header it moves per keystroke,
    // dropping keyboard. Non-scrolling flickable holds both.
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
            // Rows otherwise draw outside list box, which starts under search:
            // flick ran contacts over field.
            clip: true
            model: contacts.rows

            // No context menu: picking is the only action.
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
                // Not until core answered: see `contactsLoaded`.
                enabled: page.contactsLoaded && contacts.count === 0
                text: qsTr("No contacts yet")
                hintText: qsTr("Use \"QR code\" in the chat list to add a contact.")
            }
        }
    }
}
