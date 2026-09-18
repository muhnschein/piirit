import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piiri 1.0

/*
 * Who this profile hears nothing from. Reached from the settings page,
 * where the reference clients keep it too (deltachat-android's
 * BlockedContactsActivity, under Chats and media; deltachat-ios's
 * BlockedContactsViewController, under the same heading).
 *
 * The list is the profile's, not the phone's: the core keeps a block
 * list per account, so blocking someone on one profile leaves the other
 * profiles able to hear from them. Which profile is the one whose chats
 * the settings page was pulled down from, and it is handed in as
 * `accountId`.
 *
 * A tap on a row lets that person back in, asked about first -- the row
 * is the whole of what a thumb can hit here, and an accidental one
 * undoes a decision the reader made on purpose. Blocking someone new is
 * the pull-down, which opens the contacts to pick from
 * (BlockContactPage.qml): a blocked contact is in neither list the rest
 * of the app offers, so this page is the only way back to them.
 *
 * The words are the reference clients' own, which is also how they come
 * translated into the forty languages this ships in.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    ContactList {
        id: blockList
        objectName: "blocked"
        account_id: page.accountId
        blocked: true
        onError: page.errorMessage = message
    }

    Connections {
        target: core
        // Blocking from another device, or a contact renamed: the core
        // says contacts changed and the list is read again.
        onCore_event: blockList.handle_event(context_id, kind, payload_json)
        // A model made before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                blockList.reload()
            }
        }
    }

    // Coming back from the picker, where someone was just blocked. The
    // core announces that as well, and this is the cheap belt to its
    // braces: one call on the way back in, against a list that is short.
    onStatusChanged: {
        if (status === PageStatus.Active) {
            blockList.reload()
        }
    }

    /// Ask about letting someone back in, and do it if the reader agrees.
    function unblock(contactId, contactName) {
        var dialog = pageStack.push(Qt.resolvedUrl("BlockContactDialog.qml"), {
            contactName: contactName,
            blocking: false
        })
        if (dialog) {
            dialog.accepted.connect(function() {
                blockList.unblock(contactId)
            })
        }
    }

    // One flickable for the whole page, owning the pulley; see
    // AddMembersPage.
    SilicaFlickable {
        id: pulleyHost
        anchors.fill: parent
        contentWidth: width
        contentHeight: height

        PullDownMenu {
            MenuItem {
                objectName: "blockSomeone"
                //: Opens the contacts, to pick one to block.
                text: qsTr("Block contact")
                onClicked: pageStack.push(Qt.resolvedUrl("BlockContactPage.qml"), {
                    accountId: page.accountId
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
                title: qsTr("Blocked contacts")
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
            // starts under the header; see NewChatPage.
            clip: true
            model: blockList.rows

            delegate: ListItem {
                objectName: "blockedRow" + model.contact_id
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

                onClicked: page.unblock(model.contact_id, model.display_name)
            }

            ViewPlaceholder {
                objectName: "nobodyBlocked"
                enabled: blockList.count === 0
                // deltachat-android's `blocked_empty_hint`, as the words
                // on this page are its words throughout.
                text: qsTr("Blocked contacts will appear here.")
            }
        }
    }
}
