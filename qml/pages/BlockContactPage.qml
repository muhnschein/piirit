import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Pick someone to block: profile's contacts, searchable. From plus on blocked contacts
 * page, where reader lands after. Core lists unblocked only. Tap asks first: deliberate.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    // Empty list before core answers = unknown: see NewChatPage.
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

    Connections {
        target: core
        onCore_event: contacts.handle_event(context_id, kind, payload_json)
        // Model made before core up: nothing to load.
        onStatus_changed: {
            if (core.status === "ready") {
                contacts.reload()
            }
        }
    }

    /// Ask, block if reader agrees. Page stays for next; core hides blocked, so row goes; list
    /// behind updates from core event.
    function block(contactId, contactName) {
        var dialog = pageStack.push(Qt.resolvedUrl("BlockContactDialog.qml"), {
            contactName: contactName,
            blocking: true
        })
        if (dialog) {
            dialog.accepted.connect(function() {
                contacts.block(contactId)
            })
        }
    }

    // Search field outside list: header field would move with keystrokes.
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
                title: qsTr("Block contact")
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
            // Else rows draw outside box: see NewChatPage.
            clip: true
            model: contacts.rows

            delegate: ListItem {
                objectName: "contactRow" + model.contact_id
                contentHeight: body.height

                ContactRow {
                    id: body
                    width: parent.width
                    displayName: model.display_name
                    ownColor: model.color
                    picturePath: model.avatar_path
                    isKeyContact: model.is_key_contact
                    match: contacts.query
                }

                onClicked: page.block(model.contact_id, model.display_name)
            }

            ViewPlaceholder {
                objectName: "contactsPlaceholder"
                // Not until core answered: see `contactsLoaded`.
                enabled: page.contactsLoaded && contacts.count === 0
                text: searchField.text.trim().length > 0 ? qsTr("Nobody matches")
                                                         : qsTr("No contacts yet")
            }
        }
    }
}
