import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piiri 1.0

/*
 * Who a one-to-one chat is with: their picture, their name, the line they
 * wrote about themselves, and whether the connection is checked and
 * encrypted. Reached by swiping left from the conversation, the way the
 * group page is from a group.
 *
 * Nothing here is an address. A reader of a chatmail app has no use for
 * one, and the core's own words for a contact are the name, the picture
 * and the status line.
 *
 * The name can be the reader's own for them: the name under the picture
 * wears an edit badge, a tap on it turns the name into a field, and what
 * is typed there is what the contact is called here. Leaving it blank
 * goes back to theirs -- the core's own rule for an empty name -- and the
 * name stands there again the next time the page is seen. Applied a
 * pause after typing stops and again on the way out, the way the group
 * page renames a group.
 *
 * The same ChatInfo as the group page: a one-to-one chat has one member,
 * the contact, so the model that lists a group's members lists them. The
 * row is read into properties of the page rather than drawn from, so a
 * reload -- every save is one -- does not rebuild what the reader is
 * looking at, or the field under their thumb.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// The name the conversation page showed, until the core answers.
    property string chatName
    property string errorMessage: ""

    ChatInfo {
        id: chat
        objectName: "chat"
        account_id: page.accountId
        chat_id: page.chatId
        onError: page.errorMessage = message
        onSaved: notice.show(qsTr("Saved"))
    }

    Connections {
        target: core
        // A changed name or picture reaches here through the contact,
        // which the model reloads on.
        onCore_event: chat.handle_event(context_id, kind, payload_json)
        onStatus_changed: {
            if (core.status === "ready") {
                chat.reload()
            }
        }
    }

    /// The one member, lifted out of the row that reads it.
    property int contactId: 0
    /// The name given here, empty when none was.
    property string givenName: ""
    /// The name they chose for themselves.
    property string ownName: ""
    /// The name the core shows them under: the given one, else theirs.
    property string displayName: ""
    property string contactColor: ""
    property string avatarPath: ""
    /// What they wrote about themselves, when they did.
    property string statusLine: ""
    property bool isVerified: false
    property bool isKeyContact: true

    /// Someone has typed since the load. Guards the refill below.
    property bool edited: false
    /// The refill is writing to the field, so the change is not an edit.
    property bool filling: false

    // Filled from the core, never re-filled from it while someone is
    // typing: every save reloads, and that would reset the cursor.
    onGivenNameChanged: {
        if (!page.edited) {
            page.filling = true
            nameField.text = page.givenName
            page.filling = false
        }
    }

    // A pause, not a keystroke: a round trip per letter would be three
    // calls to write "Ada".
    Timer {
        id: autosave
        objectName: "autosave"
        interval: 1200
        onTriggered: page.applyEdits()
    }

    function applyEdits() {
        if (!chat.loaded || !page.edited || page.contactId === 0) {
            return
        }
        page.edited = false
        chat.rename_contact(page.contactId, nameField.text)
    }

    function noteEdit() {
        if (chat.loaded && !page.filling) {
            page.edited = true
            autosave.restart()
        }
    }

    // Leaving is the other moment worth saving at: a back-swipe within
    // the pause above would otherwise drop what was typed. The cursor
    // leaves the field on the way out, so the keyboard does not follow
    // the page.
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            page.applyEdits()
            nameField.done()
        }
    }

    // Every kind of thing the chat holds, on a page of its own: pushed
    // by name, as every page is, and the tile says which kind.
    function openMedia(kind) {
        pageStack.push(Qt.resolvedUrl("ChatMediaPage.qml"), {
            accountId: page.accountId,
            chatId: page.chatId,
            kind: kind
        })
    }

    // One row, the contact, read into the page: a Repeater is how a model
    // row is read from QML, and the one here has exactly one. Nothing is
    // drawn in here, so the reload behind every save rebuilds nothing on
    // screen.
    Repeater {
        model: chat.members

        delegate: Item {
            objectName: "contactDetails"
            width: 0
            height: 0

            Binding { target: page; property: "contactId"; value: model.contact_id }
            Binding { target: page; property: "givenName"; value: model.name }
            Binding { target: page; property: "ownName"; value: model.auth_name }
            Binding { target: page; property: "displayName"; value: model.display_name }
            Binding { target: page; property: "contactColor"; value: model.color }
            Binding { target: page; property: "avatarPath"; value: model.avatar_path }
            Binding { target: page; property: "statusLine"; value: model.status }
            Binding { target: page; property: "isVerified"; value: model.is_verified }
            Binding { target: page; property: "isKeyContact"; value: model.is_key_contact }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Contact")
            }

            Item {
                width: parent.width
                height: bigAvatar.height + 2 * Theme.paddingLarge

                Avatar {
                    id: bigAvatar
                    objectName: "contactAvatar"
                    anchors.centerIn: parent
                    width: 2 * Theme.itemSizeExtraLarge
                    initial: page.displayName
                    ownColor: page.contactColor
                    picturePath: page.avatarPath
                }
            }

            // What to call them here. The field holds only what was
            // given; the name they chose for themselves stands in the
            // empty field, and is what a blank field goes back to.
            EditableName {
                id: nameField
                objectName: "contactNameControl"
                fieldObjectName: "contactNameField"
                hintObjectName: "nameHint"
                fallbackText: page.ownName
                placeholderText: page.ownName.length > 0 ? page.ownName : qsTr("Name")
                hint: qsTr("Leave blank to use the name they chose")
                onTextChanged: page.noteEdit()
            }

            // What they wrote about themselves, when they did: their own
            // words, under their name, before anything this app has to
            // say about them. Pinned to plain text, being theirs.
            Label {
                objectName: "statusLabel"
                visible: page.statusLine.length > 0
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                color: Theme.primaryColor
                text: page.statusLine
            }

            // The same two facts the chat list marks a row with, said in
            // words: whether the connection is encrypted, and whether it
            // was checked in person. A caption under the person, in the
            // size and colour a caption is drawn in.
            Label {
                objectName: "encryptionLabel"
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                // Translated literals, but pinned all the same: the
                // binding reads the page to choose one.
                textFormat: Text.PlainText
                text: page.isVerified
                      ? qsTr("Verified: end-to-end encrypted, and checked in person")
                      : page.isKeyContact
                        ? qsTr("End-to-end encrypted")
                        : qsTr("Not encrypted: a plain email contact")
            }

            // What the chat holds besides words, a tile per kind, each a
            // way into the page that lists it. The row keeps its own room
            // above and below, between who they are and what the chat
            // does.
            MediaKinds {
                objectName: "mediaKinds"
                // `=== true` because dconf hands back `undefined` before it
                // has read the key.
                appsAvailable: Settings.webxdcEnabled === true
                onKindRequested: page.openMedia(kind)
            }

            DisappearingMessages {
                objectName: "disappearing"
                seconds: chat.ephemeral_timer
                canChange: chat.can_send
                onChosen: chat.set_ephemeral_timer(seconds)
            }
        }
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }

    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 2
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        onDismissed: notice.text = ""
    }
}
