import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media
import Piirit 1.0

/*
 * One-to-one chat: picture, name, status, encryption. Swipe left from
 * conversation, as group page; tap picture to enlarge. No address: core's
 * contact words only.
 *
 * Name overridable (edit badge, field); blank reverts to theirs (core's
 * rule). Saved after pause and on leave, as group renames.
 *
 * Same ChatInfo as group page; one member, the contact. Row read into
 * properties: save's reload must not rebuild field under thumb.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Name shown until core answers.
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
        // Name/picture changes arrive via contact; model reloads.
        onCore_event: chat.handle_event(context_id, kind, payload_json)
        onStatus_changed: {
            if (core.status === "ready") {
                chat.reload()
            }
        }
    }

    /// The one member, lifted from row.
    property int contactId: 0
    /// Given name, empty if none.
    property string givenName: ""
    /// Name they chose.
    property string ownName: ""
    /// Name shown: given, else theirs.
    property string displayName: ""
    property string contactColor: ""
    property string avatarPath: ""
    /// Their status line.
    property string statusLine: ""
    property bool isKeyContact: true
    /// Not seen long, or never.
    property bool seenLongAgo: false

    /// Typed since load: guards refill.
    property bool edited: false
    /// Refill writing field: not an edit.
    property bool filling: false

    // Filled from core. Not refilled while typing: reload resets cursor.
    onGivenNameChanged: {
        if (!page.edited) {
            page.filling = true
            nameField.text = page.givenName
            page.filling = false
        }
    }

    // Pause, not per keystroke: a round trip per letter.
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

    // Leaving saves too: back-swipe in pause would drop text. Cursor leaves first.
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            page.applyEdits()
            nameField.done()
        }
    }

    // Each content kind on own page, pushed by name.
    function openMedia(kind) {
        if (kind === "calls") {
            pageStack.push(Qt.resolvedUrl("CallsPage.qml"), {
                accountId: page.accountId,
                chatId: page.chatId,
                contactName: page.displayName
            })
            return
        }
        pageStack.push(Qt.resolvedUrl("ChatMediaPage.qml"), {
            accountId: page.accountId,
            chatId: page.chatId,
            kind: kind
        })
    }

    // Picture, full size. None if no picture: initial disc is not one.
    function showPicture() {
        if (page.avatarPath.length === 0) {
            return
        }
        pageStack.push(Qt.resolvedUrl("PicturePage.qml"), {
            fileUrl: Media.fileUrl(page.avatarPath),
            fileName: ""
        })
    }

    // The contact's one row, read into page (Repeater is how QML reads rows).
    // Draws nothing: reload after save rebuilds nothing on screen.
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
            Binding { target: page; property: "isKeyContact"; value: model.is_key_contact }
            Binding { target: page; property: "seenLongAgo"; value: model.seen_long_ago }
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

                MouseArea {
                    objectName: "pictureTap"
                    anchors.fill: bigAvatar
                    enabled: page.avatarPath.length > 0
                    onClicked: page.showPicture()
                }
            }

            // Name to call them here. Field holds given name only; theirs shows when
            // empty, and blank goes back to it.
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

            // Their own words about themselves. Plain text, as theirs.
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

            // Core asks: long-unseen contact may not read soon. Delta Chat's words, for
            // translations.
            Label {
                objectName: "seenLabel"
                visible: page.seenLongAgo
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                //: Under a contact's name: the core has not seen them for
                //: a long time, or never.
                text: qsTr("Seen a long time ago")
            }

            // Encryption caption under the person, in caption style.
            Label {
                objectName: "encryptionLabel"
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                // Translated literals pinned: binding reads page to pick one.
                textFormat: Text.PlainText
                text: page.isKeyContact
                      ? qsTr("End-to-end encrypted")
                      : qsTr("Not encrypted: a plain email contact")
            }

            // Tiles per kind, each a way into its listing page.
            MediaKinds {
                objectName: "mediaKinds"
                // `=== true`: dconf gives undefined before read.
                appsAvailable: Settings.webxdcEnabled === true
                callsAvailable: chat.can_call
                onKindRequested: page.openMedia(kind)
            }

            DisappearingMessages {
                objectName: "disappearing"
                seconds: chat.ephemeral_timer
                canChange: chat.can_send
                onChosen: chat.set_ephemeral_timer(seconds)
            }

            ChatNotifications {
                objectName: "notifications"
                muted: chat.is_muted
                onPicked: chat.set_mute_duration(seconds)
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
