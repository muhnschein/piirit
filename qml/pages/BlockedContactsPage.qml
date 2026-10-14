import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * Who this profile hears nothing from. Reached from settings page; reference
 * clients keep it there too (deltachat-android BlockedContactsActivity).
 *
 * Per profile: core keeps block list per account. `accountId` handed in.
 *
 * Tap lets someone back in, asked first.
 * Blocking new = plus under last row (BlockContactPage.qml). Blocked contacts
 * are in no other list, so this is the only way back to them.
 *
 * Words are reference clients' (deltachat-android/ios), so their
 * translations apply.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""

    // Empty list before core answers: unknown. See NewChatPage.
    property bool blockListLoaded: false
    // Failed answer: list blank, blocking still allowed.
    property bool blockListFailed: false

    ContactList {
        id: blockList
        objectName: "blocked"
        account_id: page.accountId
        blocked: true
        onError: {
            page.errorMessage = message
            if (!page.blockListLoaded) {
                page.blockListFailed = true
            }
        }
        // Emitted after rows set.
        onRows_changed: page.blockListLoaded = true
    }

    Connections {
        target: core
        // Blocked elsewhere or renamed: core says reread.
        onCore_event: blockList.handle_event(context_id, kind, payload_json)
        // Model made before core up: nothing to load.
        onStatus_changed: {
            if (core.status === "ready") {
                blockList.reload()
            }
        }
    }

    // Back from picker: core announces blocks; this is an extra call.
    onStatusChanged: {
        if (status === PageStatus.Active) {
            blockList.reload()
        }
    }

    /// Ask to let someone back in, if reader agrees.
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

    SilicaListView {
        id: listView
        anchors.fill: parent
        model: blockList.rows

        header: PageHeader {
            title: qsTr("Blocked contacts")
        }

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
            }

            onClicked: page.unblock(model.contact_id, model.display_name)
        }

        // Wait for core: early plus drops under rows, tap hits first row.
        // Failed answer counts: only way to block here.
        footer: PlusRow {
            objectName: "blockSomeone"
            width: listView.width
            visible: page.blockListLoaded || page.blockListFailed
            //: Opens the contacts, to pick one to block.
            text: qsTr("Block contact")
            onClicked: pageStack.push(Qt.resolvedUrl("BlockContactPage.qml"), {
                accountId: page.accountId
            })
        }

        ViewPlaceholder {
            objectName: "nobodyBlocked"
            // Not until core answered: see `blockListLoaded`.
            enabled: page.blockListLoaded && blockList.count === 0
            // deltachat-android `blocked_empty_hint` (words are theirs).
            text: qsTr("Blocked contacts will appear here.")
        }
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        text: page.errorMessage
        onDismissed: page.errorMessage = ""
    }
}
