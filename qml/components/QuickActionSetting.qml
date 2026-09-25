import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
// Settings is this directory's singleton, and Qt 5.6 only hands one out
// through an import that names the directory: without this line the name
// is the plain type, every read of it throws, and each binding on it
// keeps its default -- the phone showed a chat's controls with no chat
// action and wrote nothing at all.
import "."
import "../js/QuickActions.js" as QuickActions

/*
 * One of the cover's quick actions, as QuickActionsPage offers it: what it
 * does, and for a chat, which chat and which icon it wears.
 *
 * All of it is one choice. Choosing "Chat" opens the chat picker, and the
 * action is a chat's only once one has been picked: backing out leaves it
 * as it was. The chat can be any profile's. The choice
 * then says which chat -- "Chat: " and its name -- while the list it is
 * picked from still says "Chat", and choosing that again picks again.
 * The name is looked up rather than kept, so a renamed chat reads as it
 * is called now, and a deleted one says so. Under a chat's action, and
 * only there, are the icons it can wear.
 */
Column {
    id: setting

    /// "left" or "right", as Settings keeps them.
    property string side: "left"
    /// Whose chats the picker lists first, unless the action's chat is
    /// another profile's: the profile the settings were opened from.
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
        //: A quick action on the cover that opens the list of profiles.
        case "profiles": return qsTr("Profiles")
        //: No quick action on this side of the cover.
        default: return qsTr("None")
        }
    }

    /// What the choice says it is: a chat's action by its chat.
    function valueText() {
        if (setting.action.kind !== "chat") {
            return setting.kindLabel(setting.action.kind)
        }
        if (setting.chatGone) {
            //: What a quick action says about its chat once that chat has
            //: been deleted.
            return qsTr("Deleted chat")
        }
        if (chat.name === "") {
            return setting.kindLabel("chat")
        }
        //: A quick action on the cover that opens one chat, with the
        //: chat's name.
        return qsTr("Chat: %1").arg(chat.name)
    }

    function choose(kind) {
        if (kind === "chat") {
            setting.pickChat()
            return
        }
        Settings[setting.prefix] = kind
    }

    /// Ask which chat. Until one is picked the choice shows what it was.
    ///
    /// The picker opens on the profile the action's chat is in, or on the
    /// one the settings were opened from, and the reader can turn it to
    /// any other: the chat is whichever profile's it was picked from.
    function pickChat() {
        var picker = pageStack.push(Qt.resolvedUrl("../pages/ChatPickerPage.qml"), {
            // Only a profile there is: one the core no longer has is a
            // list of nothing but "account not found" (piirit.qml).
            accountId: setting.action.kind === "chat" && setting.action.accountId > 0
                       && core.is_configured_account(setting.action.accountId)
                       ? setting.action.accountId : setting.accountId,
            // The action's profile may have been deleted since.
            fallbackAccountId: setting.accountId,
            profileChoice: true,
            //: Over the list of chats, when picking the one a quick action
            //: on the cover opens.
            title: qsTr("Choose a chat"),
            emptyText: qsTr("No chats yet")
        })
        if (!picker) {
            return
        }
        picker.chatPicked.connect(function (chatId) {
            Settings[setting.prefix + "Account"] = picker.accountId
            Settings[setting.prefix + "Chat"] = chatId
            Settings[setting.prefix + "Icon"] = setting.action.icon
            Settings[setting.prefix] = "chat"
        })
    }

    ChatInfo {
        id: chat
        objectName: "quickActionChat"
        account_id: setting.action.accountId
        chat_id: setting.action.kind === "chat" ? setting.action.chatId : 0
        onChat_changed: setting.chatGone = false
        onError: setting.chatGone = true
    }

    // The choice: what the action is called, and what it does now, in a
    // row drawn the way Silica draws a ComboBox, with the kinds in a menu
    // that opens under it on a tap. Not a ComboBox: Silica shows one's
    // choices on a page of their own once there are more than five of
    // them, and there are six. A row's context menu opens where it is,
    // however many items it holds -- the conversation's own holds eight.
    ListItem {
        id: choice
        objectName: setting.side + "ActionChoice"
        width: parent.width
        contentHeight: Theme.itemSizeSmall
        onClicked: choice.openMenu()

        Label {
            id: choiceLabel
            objectName: setting.side + "ActionLabel"
            x: Theme.horizontalPageMargin
            anchors.verticalCenter: parent.verticalCenter
            color: choice.highlighted ? Theme.highlightColor : Theme.primaryColor
            text: setting.label
        }

        Label {
            objectName: setting.side + "ActionValue"
            anchors {
                left: choiceLabel.right
                leftMargin: Theme.paddingMedium
                right: parent.right
                rightMargin: Theme.horizontalPageMargin
                verticalCenter: parent.verticalCenter
            }
            color: Theme.highlightColor
            truncationMode: TruncationMode.Fade
            text: setting.valueText()
        }

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
            MenuItem {
                objectName: setting.side + "Action-profiles"
                text: setting.kindLabel("profiles")
                onClicked: setting.choose("profiles")
            }
        }
    }

    // The icons a chat's action can wear, the one it wears lit, in rows
    // that wrap at the page's width. Laid out by bindings rather than a
    // Grid, for the reason ChoiceTiles gives. Drawn from the same files
    // the cover hands the home screen, so what is picked here is what the
    // cover shows.
    Item {
        id: icons
        objectName: setting.side + "ActionIcons"
        visible: setting.action.kind === "chat"
        width: parent.width
        height: icons.visible ? icons.rows * icons.cell + Theme.paddingMedium : 0

        readonly property real cell: Theme.itemSizeSmall
        readonly property real inset: Theme.horizontalPageMargin - Theme.paddingMedium
        readonly property int columns: Math.max(1, Math.floor((icons.width - 2 * icons.inset)
                                                              / icons.cell))
        readonly property int rows: Math.ceil(QuickActions.chatIcons.length / icons.columns)

        Repeater {
            model: QuickActions.chatIcons

            BackgroundItem {
                objectName: setting.side + "Icon-" + modelData
                x: icons.inset + (index % icons.columns) * icons.cell
                y: Math.floor(index / icons.columns) * icons.cell
                width: icons.cell
                height: icons.cell
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
