import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media
import "../js/Calls.js" as Calls
import Piirit 1.0

/*
 * The calls with one contact, and the way to call them: behind the Calls
 * tile on the contact's page (components/MediaKinds.qml).
 *
 * A call is a message in its chat, so the chat's own index lists them
 * (chat_media.rs, kind `calls`), newest first. A row says which way the
 * call went, what became of it -- how long it lasted, or that it was
 * missed -- and when, in the chat's own words for a call (Calls.js). A
 * tap calls back, as a tap on the call's row in the chat does; one still
 * ringing here is taken up instead.
 *
 * Calling is the first row, above the calls, laid out as the phone's
 * contact card lays out its own actions: the reader who came here to
 * call does not have to read the list first.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Who the calls are with, as the contact's page names them. Theirs
    /// to choose, so drawn as plain text.
    property string contactName
    property string errorMessage: ""

    ChatMedia {
        id: media
        objectName: "media"
        account_id: page.accountId
        chat_id: page.chatId
        kind: "calls"
        onError: page.errorMessage = message
    }

    // Whether a call can be placed here, asked of the core as the
    // contact's page asks it: a contact request, or a chat that has
    // stopped being encrypted, is one the core would refuse a call in.
    ChatInfo {
        id: chat
        objectName: "chat"
        account_id: page.accountId
        chat_id: page.chatId
        onError: page.errorMessage = message
    }

    Connections {
        target: core
        // A call placed, answered or ended changes its message, which
        // the model reads again.
        onCore_event: {
            media.handle_event(context_id, kind, payload_json)
            chat.handle_event(context_id, kind, payload_json)
        }
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                media.reload()
                chat.reload()
            }
        }
    }

    /// A call can be placed from here now. `=== true` because dconf
    /// hands back `undefined` before it has read the key.
    readonly property bool canCall: Settings.callsEnabled === true && chat.can_call

    /// Call them, or go back to the call there is: one call at a time.
    function call() {
        if (typeof appWindow === "undefined") {
            return
        }
        if (!appWindow.placeCall(page.accountId, page.chatId)) {
            appWindow.showCall()
        }
    }

    /// A row was tapped. As in the chat: one still ringing here is
    /// answered, and any other is called back.
    function callFromRow(messageId, outgoing, callState) {
        if (Settings.callsEnabled !== true || typeof appWindow === "undefined") {
            return
        }
        if (!outgoing && callState === "Alerting") {
            appWindow.pickUpCall(page.accountId, page.chatId, messageId)
            return
        }
        if (page.canCall) {
            page.call()
        }
    }

    /// What a row says first: which way the call went, or -- for one
    /// that did not happen -- what became of it.
    function rowTitle(callState, outgoing) {
        if (Calls.failed(callState)) {
            return Calls.title(callState, false)
        }
        //: A call this account placed, in a list of calls.
        return outgoing ? qsTr("Outgoing call") : qsTr("Incoming call")
    }

    /// The line under it: how long it lasted, or that it is ringing.
    function rowDetail(callState, duration, outgoing) {
        if (callState === "Alerting") {
            return qsTr("Ringing…")
        }
        return Calls.detail(callState, duration, outgoing)
    }

    SilicaListView {
        id: list
        objectName: "callList"
        anchors.fill: parent
        model: media.rows

        header: Item {
            width: list.width
            height: header.height + callAction.height + Theme.paddingLarge

            PageHeader {
                id: header
                title: Media.kindName("calls")
            }

            // The contact card's own shape for an action: the icon, and
            // what it does to whom.
            BackgroundItem {
                id: callAction
                objectName: "callAction"
                y: header.height
                width: parent.width
                height: Theme.itemSizeMedium
                enabled: page.canCall
                onClicked: page.call()

                readonly property color tint: !callAction.enabled
                                              ? Theme.secondaryColor
                                              : callAction.highlighted
                                                ? Theme.highlightColor
                                                : Theme.primaryColor

                Image {
                    id: callIcon
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.iconSizeMedium
                    height: width
                    // Theme icons take their colour after the `?`.
                    source: "image://theme/icon-m-call?" + callAction.tint
                }

                Label {
                    objectName: "callActionLabel"
                    anchors {
                        left: callIcon.right
                        leftMargin: Theme.paddingMedium
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    truncationMode: TruncationMode.Fade
                    color: callAction.tint
                    // Their name, in a line of ours.
                    textFormat: Text.PlainText
                    //: Places a call. %1 is the name of whoever is called.
                    text: qsTr("Call %1").arg(page.contactName)
                }
            }
        }

        delegate: ListItem {
            id: row
            objectName: "callRow" + model.message_id
            width: list.width
            contentHeight: Theme.itemSizeMedium
            enabled: model.loaded
            onClicked: page.callFromRow(model.message_id, model.is_outgoing,
                                        model.call_state)

            readonly property bool failed: Calls.failed(model.call_state)
            readonly property string detail:
                page.rowDetail(model.call_state, model.call_duration, model.is_outgoing)

            Image {
                id: icon
                x: Theme.horizontalPageMargin
                anchors.verticalCenter: parent.verticalCenter
                width: Theme.iconSizeMedium
                height: width
                source: model.loaded
                        ? (model.call_has_video ? "image://theme/icon-m-video?"
                                                : "image://theme/icon-m-call?")
                          + (row.failed ? Theme.errorColor
                                        : row.highlighted ? Theme.highlightColor
                                                          : Theme.primaryColor)
                        : ""
            }

            Label {
                id: title
                objectName: "callTitle"
                anchors {
                    left: icon.right
                    leftMargin: Theme.paddingMedium
                    right: when.left
                    rightMargin: Theme.paddingMedium
                }
                y: row.detail.length > 0
                   ? Math.floor((row.contentHeight - height - detailLabel.height) / 2)
                   : Math.floor((row.contentHeight - height) / 2)
                truncationMode: TruncationMode.Fade
                color: row.failed ? Theme.errorColor
                                  : row.highlighted ? Theme.highlightColor
                                                    : Theme.primaryColor
                textFormat: Text.PlainText
                text: model.loaded ? page.rowTitle(model.call_state, model.is_outgoing) : ""
            }

            Label {
                id: detailLabel
                objectName: "callDetail"
                anchors {
                    left: title.left
                    right: title.right
                }
                y: title.y + title.height
                height: row.detail.length > 0 ? implicitHeight : 0
                truncationMode: TruncationMode.Fade
                font.pixelSize: Theme.fontSizeExtraSmall
                color: row.highlighted ? Theme.secondaryHighlightColor
                                       : Theme.secondaryColor
                textFormat: Text.PlainText
                text: row.detail
            }

            // When: the day, and the time of day on it.
            Label {
                id: when
                objectName: "callWhen"
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: parent.verticalCenter
                }
                font.pixelSize: Theme.fontSizeExtraSmall
                color: row.highlighted ? Theme.secondaryHighlightColor
                                       : Theme.secondaryColor
                textFormat: Text.PlainText
                text: model.loaded && model.timestamp > 0
                      ? Qt.formatDate(new Date(model.timestamp * 1000),
                                      Qt.DefaultLocaleShortDate)
                        + " " + Qt.formatTime(new Date(model.timestamp * 1000), "hh:mm")
                      : ""
            }
        }

        ViewPlaceholder {
            objectName: "placeholder"
            enabled: media.loaded && media.count === 0
            text: Media.emptyText("calls")
        }

        VerticalScrollDecorator {}
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }
}
