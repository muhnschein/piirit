import QtQuick 2.0
import Nemo.Notifications 1.0
import Nemo.DBus 2.0

/*
 * Raises a notification for a message that arrives while the reader is not
 * looking at the chat it landed in, and takes it back down when they are.
 *
 * One notification per chat, kept and reused rather than replaced, so two
 * chats talking at once do not overwrite each other in the notification
 * area -- the real Notification allocates an id on first publish and
 * `replacesId` reuses it. A second message in the same chat counts up on
 * the one notification rather than adding another.
 *
 * `ChatList` decides what counts as an arrival and never announces a muted
 * chat; this decides only whether the reader was there to see it, and how
 * much to say: the lock screen shows a notification to whoever is looking
 * at it, so `detail` follows the setting for how much it gives away.
 *
 * A tap on a notification comes back over D-Bus. Sailjail lets the app
 * own the name made of its desktop file's OrganizationName and
 * ApplicationName, which is `postivene.postivene`; the adaptor below owns
 * it and the notification's remote action names it, so lipstick's call
 * lands here and is passed on as `openRequested`.
 */
Item {
    id: notifier

    /// The chat on screen right now, 0 for none.
    property int viewingChatId: 0
    /// False while the app is in the background, where "on screen" is not
    /// the same as "seen".
    property bool appActive: Qt.application.state === Qt.ApplicationActive
    /// How much a notification says: 0 the chat and the message, 1 the
    /// chat only, 2 only that something arrived. The page binds it to the
    /// setting; the component itself reads no setting, so it can be
    /// loaded on its own.
    property int detail: 0

    /// A notification was tapped: the chat to open.
    signal openRequested(int chatId)

    // `enabled` is the Item's own, put to use: off, nothing arriving is
    // announced and whatever is up comes down, since a reader who turned
    // notifications off did not mean the ones already there to stay. The
    // page binds it to the setting; the component itself reads none.
    onEnabledChanged: {
        if (!enabled) {
            notifier.clearAll()
        }
    }

    /// chatId -> Notification.
    property var notes: ({})
    /// chatId -> the chat's name, kept apart from the notification, which
    /// does not always say it.
    property var names: ({})
    /// chatId -> messages announced since the notification went up.
    property var counts: ({})

    /// The D-Bus name a tap calls back to; see the note above. One string
    /// for the service and the interface, as the reference for Nemo's
    /// remote actions shows it.
    readonly property string busName: "postivene.postivene"
    readonly property string busPath: "/"

    // Reading a chat is the answer to "have I seen this", so drop the
    // notification the moment the reader arrives.
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
        xml: "  <interface name=\"postivene.postivene\">\n" +
             "    <method name=\"showChat\">\n" +
             "      <arg name=\"chatId\" type=\"i\" direction=\"in\"/>\n" +
             "    </method>\n" +
             "  </interface>\n"

        /// What the tap calls. The chat id is the notification's own
        /// argument, so a notification that outlived the app's memory of
        /// it still names the chat.
        function showChat(chatId) {
            notifier.openRequested(chatId)
        }
    }

    property Component noteComponent: Component {
        Notification {
            category: "x-nemo.messaging.im"
            appName: "Postivene"
            appIcon: "harbour-postivene"
        }
    }

    /// The name of the chat behind a notification, empty when there is
    /// none. What a tap opens the chat under before the core says.
    function nameOf(chatId) {
        var name = notifier.names[chatId]
        return name ? name : ""
    }

    /// What the notification says, at the detail the reader chose.
    /// `sender` is who wrote it in a group; in a one-to-one chat the chat
    /// is already named after them and the sender comes in empty.
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

    /// Announce a message, unless the reader is already looking at it.
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
        // Both the banner and the entry that stays in the notification
        // area: the previewed pair is what appears over the top.
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

    /// Take down whatever is standing for this chat.
    function clear(chatId) {
        var note = notifier.notes[chatId]
        if (note) {
            note.close()
        }
        notifier.counts[chatId] = 0
    }

    /// How many chats are currently speaking for themselves. For tests.
    /// Every notification down, and every count back to nothing.
    function clearAll() {
        for (var id in notifier.notes) {
            notifier.clear(parseInt(id, 10))
        }
    }

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
