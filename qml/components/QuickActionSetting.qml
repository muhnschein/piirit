import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../js/QuickActions.js" as QuickActions

/*
 * One of the cover's two quick actions, as the settings page offers it:
 * what it does, and for a chat, which chat and which icon it wears.
 *
 * A chat is picked on the chat picker, from the profile the settings were
 * opened from. Choosing "Chat" opens the picker, and the action is a
 * chat's only once one has been picked: backing out leaves it as it was.
 * The chat's name is looked up rather than kept, so a renamed chat reads
 * as it is called now, and a deleted one says so.
 */
Column {
    id: setting

    /// "left" or "right", as Settings keeps them.
    property string side: "left"
    /// Whose chats the picker lists.
    property int accountId
    /// What the choice is called.
    property string label

    readonly property string prefix: setting.side === "right" ? "quickActionRight"
                                                              : "quickActionLeft"
    readonly property var action: QuickActions.read(Settings, setting.side)

    /// The action's chat has gone since it was picked.
    property bool chatGone: false

    width: parent ? parent.width : 0

    function kindLabel(kind) {
        switch (kind) {
        //: A quick action on the cover that opens one chat.
        case "chat": return qsTr("Chat")
        //: A quick action on the cover that opens the chat list's search.
        case "search": return qsTr("Search")
        //: A quick action on the cover that shows this profile's QR code.
        case "qr": return qsTr("My QR code")
        //: A quick action on the cover that opens the QR code scanner.
        case "scan": return qsTr("Scan QR code")
        //: No quick action on this side of the cover.
        default: return qsTr("None")
        }
    }

    /// Put the choice back to what the setting holds. Silica moves it on
    /// the tap, which detaches a binding; see SettingsPage.
    function refresh() {
        combo.currentIndex = QuickActions.kinds.indexOf(setting.action.kind) + 1
    }

    function choose(kind) {
        if (kind === "chat") {
            setting.pickChat()
            return
        }
        Settings[setting.prefix] = kind
    }

    /// Ask which chat. Until one is picked the choice shows what it was.
    function pickChat() {
        setting.refresh()
        var picker = pageStack.push(Qt.resolvedUrl("../pages/ChatPickerPage.qml"), {
            accountId: setting.accountId,
            //: Over the list of chats, when picking the one a quick action
            //: on the cover opens.
            title: qsTr("Choose a chat"),
            emptyText: qsTr("No chats yet")
        })
        if (!picker) {
            return
        }
        picker.chatPicked.connect(function (chatId) {
            Settings[setting.prefix + "Account"] = setting.accountId
            Settings[setting.prefix + "Chat"] = chatId
            Settings[setting.prefix + "Icon"] = setting.action.icon
            Settings[setting.prefix] = "chat"
        })
    }

    Connections {
        target: Settings
        onQuickActionLeftChanged: setting.refresh()
        onQuickActionRightChanged: setting.refresh()
    }

    Component.onCompleted: setting.refresh()

    ChatInfo {
        id: chat
        objectName: "quickActionChat"
        account_id: setting.action.accountId
        chat_id: setting.action.kind === "chat" ? setting.action.chatId : 0
        onChat_changed: setting.chatGone = false
        onError: setting.chatGone = true
    }

    ComboBox {
        id: combo
        objectName: setting.side + "ActionCombo"
        width: parent.width
        label: setting.label

        // In the order of QuickActions.kinds, after "None": `refresh`
        // counts on it.
        menu: ContextMenu {
            MenuItem {
                objectName: setting.side + "Action-none"
                text: setting.kindLabel("")
                onClicked: setting.choose("")
            }
            MenuItem {
                objectName: setting.side + "Action-chat"
                text: setting.kindLabel("chat")
                onClicked: setting.choose("chat")
            }
            MenuItem {
                objectName: setting.side + "Action-search"
                text: setting.kindLabel("search")
                onClicked: setting.choose("search")
            }
            MenuItem {
                objectName: setting.side + "Action-qr"
                text: setting.kindLabel("qr")
                onClicked: setting.choose("qr")
            }
            MenuItem {
                objectName: setting.side + "Action-scan"
                text: setting.kindLabel("scan")
                onClicked: setting.choose("scan")
            }
        }
    }

    ValueButton {
        objectName: setting.side + "ActionChatButton"
        visible: setting.action.kind === "chat"
        label: setting.kindLabel("chat")
        //: What a quick action says about its chat once that chat has
        //: been deleted.
        value: setting.chatGone ? qsTr("Deleted chat") : chat.name
        onClicked: setting.pickChat()
    }

    // The icons a chat's action can wear, the one it wears lit. Drawn from
    // the same files the cover hands the home screen, so what is picked
    // here is what the cover shows.
    Row {
        objectName: setting.side + "ActionIcons"
        visible: setting.action.kind === "chat"
        x: Theme.horizontalPageMargin - Theme.paddingMedium

        Repeater {
            model: QuickActions.chatIcons

            BackgroundItem {
                objectName: setting.side + "Icon-" + modelData
                width: Theme.itemSizeSmall
                height: Theme.itemSizeSmall
                highlighted: down || setting.action.icon === modelData
                onClicked: Settings[setting.prefix + "Icon"] = modelData

                Image {
                    anchors.centerIn: parent
                    source: Qt.resolvedUrl("../" + QuickActions.iconFile(
                        modelData, Theme.iconSizeSmall,
                        QuickActions.isLight(Theme.primaryColor)))
                }
            }
        }
    }
}
