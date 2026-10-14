import QtQuick 2.0
import Nemo.Notifications 1.0
import Nemo.DBus 2.0

/*
 * Notifies arrivals the reader is not looking at; takes them down once they are.
 *
 * One notification per chat, reused (`replacesId`); same-chat messages count up.
 * Edits update it; a deletion drops it or shows the message before. No banner then.
 *
 * ChatList decides arrivals and skips muted chats. Here: was reader there, and how much
 * to say (lock screen shows to anyone; `detail` follows the setting).
 *
 * Taps return over D-Bus: Sailjail lets the app own OrganizationName.ApplicationName from
 * its desktop file, `piirit.piirit`. Adaptor below owns it, notification's remote action
 * names it; lipstick's call lands here as `openRequested`.
 */
Item {
    id: notifier

    /// Chat on screen; 0 for none.
    property int viewingChatId: 0
    /// False in background: on screen is not seen.
    property bool appActive: Qt.application.state === Qt.ApplicationActive
    /// Detail: 0 chat and message, 1 chat only, 2 arrival only. Page binds it; component
    /// reads no setting, so it loads alone.
    property int detail: 0

    /// Tapped: chat to open.
    signal openRequested(int chatId)

    // `enabled`: off means nothing announced and what is up comes down. Page binds it.
    onEnabledChanged: {
        if (!enabled) {
            notifier.clearAll()
        }
    }

    /// chatId -> Notification.
    property var notes: ({})
    /// chatId -> chat name, kept since notification may not say it.
    property var names: ({})
    /// chatId -> messages announced since notification up.
    property var counts: ({})

    /// D-Bus name for tap callbacks (see header); one string for service and interface,
    /// as Nemo's remote-action reference does.
    readonly property string busName: "piirit.piirit"
    readonly property string busPath: "/"

    // Reading a chat means seen: drop its notification.
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

        /// Tap's call. Chat id is the notification's own argument, so one outliving the
        /// app's memory still names its chat.
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

    /// Chat name for a notification, "" for none: title a tap opens the chat under until
    /// core says.
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

    /// Announce message unless reader is looking at it.
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
        // Banner and entry: the previewed pair pops up.
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

    /// Restate after edit/delete. `unread` now; zero takes it down. Nothing raised for a
    /// chat with none up; nothing changed if it already says this.
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
        // No banner: nothing arrived, so no preview pops up.
        note.previewSummary = ""
        note.previewBody = ""
        note.itemCount = count
        note.publish()
    }

    /// Take down this chat's notification.
    function clear(chatId) {
        var note = notifier.notes[chatId]
        if (note) {
            note.close()
        }
        notifier.counts[chatId] = 0
    }

    /// Every notification down, counts to zero.
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
