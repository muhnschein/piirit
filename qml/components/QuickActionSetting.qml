import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
// Qt 5.6: Settings singleton needs import naming this dir. Without it, reads
// throw and bindings keep defaults (phone showed chat controls with no chat
// action, saved nothing).
import "."
import "../js/QuickActions.js" as QuickActions

/*
 * Cover quick action (QuickActionsPage). Chat action adds chat and icon.
 *
 * One choice. "Chat" opens picker; action becomes a chat's only once one is
 * picked (back out keeps old). Any profile's chat. Choice then says "Chat:
 * <name>"; list still says "Chat", choosing it again re-picks. Name looked up,
 * not stored: renames show, deletion says so. Icons only under chat action.
 *
 * Camera actions ask chat after capture. Only under those: send at once vs wait
 * in chat for caption. Wait is default (as share) and is reset on re-choosing;
 * send-at-once only after a warning about mis-tapped chats. Choice is Silica
 * ComboBox (lists kinds on own page).
 */
Column {
    id: setting

    /// "left" or "right".
    property string side: "left"
    /// Picker's first profile: settings' one, unless chat's is another.
    property int accountId
    /// Choice label.
    property string label
    /// Page owning picker; picker waits for it on screen.
    property Item hostPage

    readonly property string prefix: setting.side === "right" ? "quickActionRight"
                                                              : "quickActionLeft"
    readonly property var action: QuickActions.read(Settings, setting.side)

    /// Chat deleted since pick.
    property bool chatGone: false

    /// The kinds, in the order the choice lists them; "" is none.
    readonly property var kinds: ["", "chat", "search", "qr", "scan", "profiles",
                                  "photo", "video", "selfie", "selfie-video"]

    /// Choice text.
    readonly property string shown: setting.valueText()
    onShownChanged: choice.value = setting.shown

    /// "Chat" chosen. Picker waits for Silica's choice page to close.
    property bool chatWanted: false

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
        //: A quick action on the cover that opens the camera to take a
        //: photo with the back camera, then asks which chat to send it to.
        case "photo": return qsTr("Photo")
        //: A quick action on the cover that opens the camera to record a
        //: video with the back camera, then asks which chat to send it to.
        case "video": return qsTr("Video")
        //: A quick action on the cover that opens the camera to take a
        //: photo with the front camera, then asks which chat to send it to.
        case "selfie": return qsTr("Selfie")
        //: A quick action on the cover that opens the camera to record a
        //: video with the front camera, then asks which chat to send it to.
        case "selfie-video": return qsTr("Selfie video")
        //: No quick action on this side of the cover.
        default: return qsTr("None")
        }
    }

    /// Chat action named by its chat.
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
            // Silica moves choice to tapped item; keep value until pick.
            setting.refresh()
            setting.chatWanted = true
            setting.openWantedPicker()
            return
        }
        // Send-at-once stays off until user re-enables it knowingly.
        Settings[setting.prefix + "SendNow"] = false
        Settings[setting.prefix] = kind
    }

    /// Off at once. On after user warned about mis-tapped chat and says go.
    function toggleSendNow() {
        if (setting.action.sendNow) {
            Settings[setting.prefix + "SendNow"] = false
            return
        }
        var dialog = pageStack.push(Qt.resolvedUrl("../pages/ExperimentalFeatureDialog.qml"), {
            title: sendNow.text,
            //: Asked before "Send right away" is turned on for a camera
            //: quick action on the cover.
            text: qsTr("The photo or video goes out the moment you tap a chat, with no chance to look it over first. Tap the wrong chat by mistake, and it goes there.")
        })
        if (dialog) {
            dialog.accepted.connect(function () {
                Settings[setting.prefix + "SendNow"] = true
            })
        }
    }

    /// Reset to setting's value; Silica's tap detaches currentIndex binding.
    function refresh() {
        choice.currentIndex = Math.max(0, setting.kinds.indexOf(setting.action.kind))
    }

    readonly property string kind: setting.action.kind
    onKindChanged: setting.refresh()
    Component.onCompleted: setting.refresh()

    /// Ask chat. Shows old choice until picked.
    /// Picker opens on action's chat profile, else settings'; user may switch.
    function pickChat() {
        var picker = pageStack.push(Qt.resolvedUrl("../pages/ChatPickerPage.qml"), {
            // Missing profile lists only "account not found" (piirit.qml).
            accountId: setting.action.kind === "chat" && setting.action.accountId > 0
                       && core.is_configured_account(setting.action.accountId)
                       ? setting.action.accountId : setting.accountId,
            // Action's profile may be deleted.
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

    /// Ask once page on screen, nothing moving. Choice tap may leave Silica's
    /// page on stack; picker pushed now would be dropped with it.
    function openWantedPicker() {
        if (!setting.chatWanted || !setting.hostPage
                || setting.hostPage.status !== PageStatus.Active || pageStack.busy) {
            return
        }
        setting.chatWanted = false
        setting.pickChat()
    }

    Connections {
        target: setting.hostPage
        onStatusChanged: setting.openWantedPicker()
    }

    Connections {
        target: pageStack
        onBusyChanged: setting.openWantedPicker()
    }

    ChatInfo {
        id: chat
        objectName: "quickActionChat"
        account_id: setting.action.accountId
        chat_id: setting.action.kind === "chat" ? setting.action.chatId : 0
        onChat_changed: setting.chatGone = false
        onError: setting.chatGone = true
    }

    // Stock Silica ComboBox (lists kinds on own page). Text is setting's, not
    // tapped item's, so chat action names chat.
    ComboBox {
        id: choice
        objectName: setting.side + "ActionChoice"
        width: parent.width
        label: setting.label
        value: setting.shown
        // Silica writes tapped text here ("Chat" if unpicked): put back.
        onValueChanged: {
            if (choice.value !== setting.shown) {
                choice.value = setting.shown
            }
        }

        menu: ContextMenu {
            Repeater {
                model: setting.kinds

                MenuItem {
                    objectName: setting.side + "Action-" + (modelData || "none")
                    text: setting.kindLabel(modelData)
                    onClicked: setting.choose(modelData)
                }
            }
        }
    }

    // Camera action: send at once on pick. Only these send anything.
    TextSwitch {
        id: sendNow
        objectName: setting.side + "ActionSendNow"
        visible: QuickActions.isCapture(setting.action.kind)
        //: Under a quick action on the cover that takes a photo or a
        //: video: whether it is sent as soon as a chat is picked for it.
        text: qsTr("Send right away")
        //: Under "Send right away". Off, the chat opens with the photo or
        //: video on it, ready for a caption and the send button.
        description: qsTr("Sends it as soon as you pick the chat, without a caption.")
        checked: setting.action.sendNow
        automaticCheck: false
        onClicked: setting.toggleSendNow()
    }

    // Icons chat action can wear; lit = current. Bindings-laid rows, not Grid
    // (see ChoiceTiles). Same files as cover, so picked = shown.
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
