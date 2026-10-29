import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0

/*
 * Map of one chat's locations, or every chat's: Delta Chat's maps app, readied in core
 * (`maps.rs`), run by WebxdcPage. Pushed by URL: `Sailfish.WebView` resolves only where engine
 * installed. Failure on `error`.
 */
LocationMap {
    id: root

    /// Chat whose map opens; empty for every chat's.
    property string chatName: ""

    /// Chat 0: every chat. Message with own point: map there; else on sender, if they shared.
    function show(chatId, contactId, messageId) {
        root.open(chatId, contactId, messageId)
    }

    onOpened: pageStack.push(Qt.resolvedUrl("../pages/WebxdcPage.qml"), {
        accountId: root.account_id,
        messageId: message_id,
        query: query,
        //: Heading of the map of every chat's shared locations.
        heading: root.chatName.length > 0 ? root.chatName : qsTr("All locations"),
        //: Under a chat's name above the map of its shared locations.
        subheading: root.chatName.length > 0 ? qsTr("Map") : ""
    })
}
