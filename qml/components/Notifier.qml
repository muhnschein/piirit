import QtQuick 2.0
import Nemo.Notifications 1.0
import Nemo.DBus 2.0

/*
 * Notifies unseen arrivals; withdraws once seen. One per chat, reused (`replacesId`),
 * counting up. Edit updates it; deletion drops it or shows prior message, no banner.
 * ChatList decides arrivals, skips muted. Here: was reader there, how much to say (lock
 * screen shows anyone; `detail` follows setting).
 *
 * Taps return over D-Bus: Sailjail lets app own OrganizationName.ApplicationName from
 * desktop file, `piirit.piirit`. Adaptor below owns it, remote action names it;
 * lipstick's call lands as `openRequested`.
 */
Item {
    id: notifier

    /// Chat on screen; 0 for none.
    property int viewingChatId: 0
    /// False in background: on screen isn't seen.
    property bool appActive: Qt.application.state === Qt.ApplicationActive
    /// 0 chat and message, 1 chat only, 2 arrival only. Page binds it: no setting read here,
    /// loads alone.
    property int detail: 0

    /// Tapped: chat to open.
    signal openRequested(int chatId)

    // `enabled` off: nothing announced, shown ones withdrawn. Page binds it.
    onEnabledChanged: {
        if (!enabled) {
            notifier.clearAll()
        }
    }

    /// chatId -> Notification.
    property var notes: ({})
    /// chatId -> chat name (notification may not say it).
    property var names: ({})
    /// chatId -> messages announced since notification up.
    property var counts: ({})

    /// Tap D-Bus name (header); service and interface alike, as Nemo's remote-action reference.
    readonly property string busName: "piirit.piirit"
    readonly property string busPath: "/"

    // Read chat = seen: drop its notification.
    onViewingChatIdChanged: {
        if (viewingChatId !== 0 && appActive) {
            notifier.clear(viewingChatId)
        }
    }
    onAppActiveChanged: {
        if (appActive && viewingChatId !== 0) {
            notifier.clear(viewingChatId)
        }
    }

    DBusAdaptor {
        id: adaptor
        objectName: "notifierAdaptor"
        service: notifier.busName
        path: notifier.busPath
        iface: notifier.busName
        xml: "  <interface name=\"piirit.piirit\">\n" +
             "    <method name=\"showChat\">\n" +
             "      <arg name=\"chatId\" type=\"i\" direction=\"in\"/>\n" +
             "    </method>\n" +
             "  </interface>\n"

        /// Tap's call. Chat id is notification's own argument: survives app restart.
        function showChat(chatId) {
            notifier.openRequested(chatId)
        }
    }

    property Component noteComponent: Component {
        Notification {
            category: "x-nemo.messaging.im"
            appName: "Piirit"
            appIcon: "harbour-piirit"
        }
    }

    /// Chat name, "" none: title tap opens chat under until core says.
    function nameOf(chatId) {
        var name = notifier.names[chatId]
        return name ? name : ""
    }

    /// Notification text at detail. `sender` only in groups.
    function wording(chatName, sender, preview, count) {
        if (notifier.detail >= 2) {
            //: A notification that says no more than this. %n is how many arrived in one chat.
            return { summary: qsTr("%n new message(s)", "", count), body: "" }
        }
        if (notifier.detail === 1) {
            return { summary: chatName, body: qsTr("%n new message(s)", "", count) }
        }
        return {
            summary: chatName,
            body: sender.length > 0 ? sender + ": " + preview : preview
        }
    }

    /// Announce message unless seen.
    function arrived(chatId, chatName, sender, preview) {
        if (!notifier.enabled) {
            return
        }
        if (appActive && chatId === viewingChatId) {
            return
        }
        var note = notifier.notes[chatId]
        if (!note) {
            note = noteComponent.createObject(notifier)
            if (!note) {
                return
            }
            notifier.notes[chatId] = note
        }
        notifier.names[chatId] = chatName
        var count = (notifier.counts[chatId] || 0) + 1
        notifier.counts[chatId] = count
        var says = notifier.wording(chatName, sender, preview, count)
        // Banner and entry: previewed pair pops up.
        note.summary = says.summary
        note.body = says.body
        note.previewSummary = says.summary
        note.previewBody = says.body
        note.itemCount = count
        note.timestamp = new Date()
        note.remoteActions = [{
            "name": "default",
            "service": notifier.busName,
            "path": notifier.busPath,
            "iface": notifier.busName,
            "method": "showChat",
            "arguments": [chatId]
        }]
        note.publish()
    }

    /// Restate after edit/delete with `unread` now; zero withdraws. None up: nothing raised;
    /// same text: unchanged.
    function restated(chatId, chatName, sender, preview, unread) {
        var note = notifier.notes[chatId]
        var shown = notifier.counts[chatId] || 0
        if (!note || shown === 0) {
            return
        }
        if (unread === 0) {
            notifier.clear(chatId)
            return
        }
        var count = Math.min(shown, unread)
        var says = notifier.wording(chatName, sender, preview, count)
        if (says.summary === note.summary && says.body === note.body
                && count === shown) {
            return
        }
        notifier.names[chatId] = chatName
        notifier.counts[chatId] = count
        note.summary = says.summary
        note.body = says.body
        // No banner: nothing arrived.
        note.previewSummary = ""
        note.previewBody = ""
        note.itemCount = count
        note.publish()
    }

    /// Withdraw chat's notification.
    function clear(chatId) {
        var note = notifier.notes[chatId]
        if (note) {
            note.close()
        }
        notifier.counts[chatId] = 0
    }

    /// Withdraw all, counts zeroed.
    function clearAll() {
        for (var id in notifier.notes) {
            notifier.clear(parseInt(id, 10))
        }
    }

    /// Chats with a notification up. For tests.
    function publishedCount() {
        var total = 0
        for (var id in notifier.notes) {
            if (notifier.notes[id].isPublished) {
                total += 1
            }
        }
        return total
    }
}
