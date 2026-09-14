import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Postivene 1.0

/*
 * Pick somebody to block: this profile's contacts, and a search over
 * them. Reached from the pull-down on the blocked contacts page, which
 * is where the reader ends up again once someone is picked.
 *
 * Only the contacts that are not blocked already are here -- that is the
 * list the core gives, and the blocked ones are on the page that opened
 * this. A tap asks first: blocking is done on purpose, and a tap in a
 * list of names is easy to make by accident.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: page.errorMessage = message
    }

    // A keystroke's worth of quiet before the core is asked, so typing a
    // name is one search rather than one per letter.
    Timer {
        id: searchDebounce
        objectName: "searchDebounce"
        interval: 250
        onTriggered: contacts.query = searchField.text.trim()
    }

    Connections {
        target: core
        onCore_event: contacts.handle_event(context_id, kind, payload_json)
        // A model made before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                contacts.reload()
            }
        }
    }

    /// Ask about blocking someone, and block them if the reader agrees.
    ///
    /// The page stays where it is afterwards: the row goes -- the core
    /// keeps a blocked contact out of this list -- which says it took,
    /// and a reader blocking two people in a row does not have to come
    /// back in for the second. The list behind this one has them by then,
    /// since the core announces the change.
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

    // The search field outside the list, for the reason NewChatPage
    // documents: a field in a view's header lives inside the flickable
    // and moves on every keystroke.
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
            // Rows draw outside the list's own box otherwise, and the box
            // starts under the search field; see NewChatPage.
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
                    isVerified: model.is_verified
                }

                onClicked: page.block(model.contact_id, model.display_name)
            }

            ViewPlaceholder {
                enabled: contacts.count === 0
                text: searchField.text.trim().length > 0 ? qsTr("Nobody matches")
                                                         : qsTr("No contacts yet")
            }
        }
    }
}
