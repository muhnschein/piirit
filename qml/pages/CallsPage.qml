import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Media.js" as Media
import "../js/Calls.js" as Calls
import "../js/Emoji.js" as Emoji
import Piirit 1.0

/*
 * Calls with one contact, behind Calls tile (components/MediaKinds.qml).
 *
 * Call messages (chat_media.rs, kind `calls`), newest first, wording from
 * Calls.js. Tap calls back (ringing: answers). Call row first.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Contact name as contact page; plain text (theirs).
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

    // Core refuses calls to requests and unencrypted chats.
    ChatInfo {
        id: chat
        objectName: "chat"
        account_id: page.accountId
        chat_id: page.chatId
        onError: page.errorMessage = message
    }

    Connections {
        target: core
        // Call changes its message: reread.
        onCore_event: {
            media.handle_event(context_id, kind, payload_json)
            chat.handle_event(context_id, kind, payload_json)
        }
        // No core yet: nothing to load.
        onStatus_changed: {
            if (core.status === "ready") {
                media.reload()
                chat.reload()
            }
        }
    }

    /// Can place call now.
    readonly property bool canCall: chat.can_call

    /// Call back (video: camera on), or open live call.
    function call(video) {
        if (typeof appWindow === "undefined") {
            return
        }
        if (!appWindow.placeCall(page.accountId, page.chatId, video === true)) {
            appWindow.showCall()
        }
    }

    /// Tap: answer ringing, else call back.
    function callFromRow(messageId, outgoing, callState, hasVideo) {
        if (typeof appWindow === "undefined") {
            return
        }
        if (!outgoing && callState === "Alerting") {
            appWindow.pickUpCall(page.accountId, page.chatId, messageId)
            return
        }
        if (page.canCall) {
            page.call(hasVideo)
        }
    }

    /// Row line 1: direction, or failed call's fate.
    function rowTitle(callState, outgoing) {
        if (Calls.failed(callState)) {
            return Calls.title(callState, false)
        }
        //: A call this account placed, in a list of calls.
        return outgoing ? qsTr("Outgoing call") : qsTr("Incoming call")
    }

    /// Row line 2: duration, or ringing.
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
            // Gap: call row isn't a call.
            height: header.height + callAction.height + videoAction.height
                    + 3 * Theme.paddingLarge

            PageHeader {
                id: header
                title: Media.kindName("calls")
            }

            // Contact card action: icon and effect.
            BackgroundItem {
                id: callAction
                objectName: "callAction"
                y: header.height
                width: parent.width
                height: Theme.itemSizeMedium
                enabled: page.canCall
                onClicked: page.call(false)

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
                    // Theme icon colour after `?`.
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
                    // Their name in our line; emoji as Twemoji (Emoji.js).
                    //: Places a call. %1 is the name of whoever is called.
                    readonly property string plain: qsTr("Call %1").arg(page.contactName)
                    readonly property string pictured: Emoji.inText(plain, false,
                                                                    Theme.fontSizeMedium,
                                                                    "", Emoji.nameSaturation)
                    textFormat: pictured.length > 0 ? Text.StyledText : Text.PlainText
                    text: pictured.length > 0 ? pictured : plain
                }
            }

            // Same, camera on.
            BackgroundItem {
                id: videoAction
                objectName: "videoCallAction"
                y: callAction.y + callAction.height
                width: parent.width
                height: Theme.itemSizeMedium
                enabled: page.canCall
                onClicked: page.call(true)

                readonly property color tint: !videoAction.enabled
                                              ? Theme.secondaryColor
                                              : videoAction.highlighted
                                                ? Theme.highlightColor
                                                : Theme.primaryColor

                Image {
                    id: videoIcon
                    x: Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.iconSizeMedium
                    height: width
                    source: "image://theme/icon-m-video?" + videoAction.tint
                }

                Label {
                    objectName: "videoCallActionLabel"
                    anchors {
                        left: videoIcon.right
                        leftMargin: Theme.paddingMedium
                        right: parent.right
                        rightMargin: Theme.horizontalPageMargin
                        verticalCenter: parent.verticalCenter
                    }
                    truncationMode: TruncationMode.Fade
                    color: videoAction.tint
                    //: Places a video call. %1 is the name of whoever is called.
                    readonly property string plain: qsTr("Video call %1").arg(page.contactName)
                    readonly property string pictured: Emoji.inText(plain, false,
                                                                    Theme.fontSizeMedium,
                                                                    "", Emoji.nameSaturation)
                    textFormat: pictured.length > 0 ? Text.StyledText : Text.PlainText
                    text: pictured.length > 0 ? pictured : plain
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
                                        model.call_state, model.call_has_video)

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

            // When: day, time of day.
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
