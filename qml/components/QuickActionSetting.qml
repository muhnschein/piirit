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
 *
 * The camera's actions -- a photo or a video, with the back camera or the
 * front -- ask which chat after the picture is taken, not here. Under
 * one of those, and only there, is whether it then goes at once or waits
 * in the chat for a caption and the send button. Waiting is the default,
 * as a share waits, and what a camera action is chosen with anew: going
 * at once is turned on only past a word on what a chat tapped by mistake
 * then gets.
 *
 * The choice is Silica's own ComboBox, which lists this many kinds on a
 * page of their own.
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
    /// The page this is on: the chat picker waits for it to be on screen.
    property Item hostPage

    readonly property string prefix: setting.side === "right" ? "quickActionRight"
                                                              : "quickActionLeft"
    readonly property var action: QuickActions.read(Settings, setting.side)

    /// The action's chat has gone since it was picked.
    property bool chatGone: false

    /// The kinds, in the order the choice lists them; "" is none.
    readonly property var kinds: ["", "chat", "search", "qr", "scan", "profiles",
                                  "photo", "video", "selfie", "selfie-video"]

    /// What the choice says.
    readonly property string shown: setting.valueText()
    onShownChanged: choice.value = setting.shown

    /// "Chat" was chosen, and the picker waits for Silica's page of
    /// choices to have gone (`openWantedPicker`).
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
            // Silica has already moved the choice to what was tapped; it
            // stays on what the setting holds until a chat is picked.
            setting.refresh()
            setting.chatWanted = true
            setting.openWantedPicker()
            return
        }
        // Whatever the action was set to do with what it takes, a camera
        // action chosen now sends nothing on its own until that is turned
        // on again, knowingly.
        Settings[setting.prefix + "SendNow"] = false
        Settings[setting.prefix] = kind
    }

    /// Off at once; on only once the reader has been told what it means
    /// for a chat tapped by mistake, and has said go ahead -- the
    /// arrangement the experimental features' switches use.
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

    /// Put the choice back to what the setting holds. Silica writes
    /// currentIndex itself on a tap, which detaches a binding -- the
    /// arrangement DisappearingMessages uses.
    function refresh() {
        choice.currentIndex = Math.max(0, setting.kinds.indexOf(setting.action.kind))
    }

    readonly property string kind: setting.action.kind
    onKindChanged: setting.refresh()
    Component.onCompleted: setting.refresh()

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

    /// Ask for the chat once this page is back on screen with nothing
    /// moving. The choice is tapped on Silica's page of choices, which can
    /// still be on the stack, or on its way off it, when the tap is
    /// reported: a picker pushed then would go on top of it, and be taken
    /// down with it.
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

    // The choice: what the action is called, and what it does now. Stock
    // Silica: with this many kinds, its ComboBox lists them on a page of
    // their own. What it says is the setting's, not the item tapped, so a
    // chat's action says which chat.
    ComboBox {
        id: choice
        objectName: setting.side + "ActionChoice"
        width: parent.width
        label: setting.label
        value: setting.shown
        // Silica writes the item's own text in here when one is tapped,
        // which says "Chat" for a chat that is not picked yet: put back.
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

    // Whether a camera action's picture or video goes as soon as its chat
    // is picked. Only under one of those: no other action sends anything.
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
