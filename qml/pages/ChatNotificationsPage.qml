import QtQuick 2.0
import Sailfish.Silica 1.0
import Piirit 1.0
import "../js/Mute.js" as Mute

/*
 * Chat's notifications: on, or off for how long. Active choice marked, with time left.
 * Swipe back saves, once, if changed. Muted with end not known (other device): none marked.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Core's word: muted now.
    property bool muted: false
    /// Core's `notify_mentions`: replies still notify. Caller decides.
    property bool mentionsStillNotify: false

    /// Leaving with another choice: seconds, -1 forever, 0 on.
    signal picked(int seconds)

    MuteState {
        id: kept
        objectName: "keptMute"
        account_id: page.accountId
        chat_id: page.chatId
        muted: page.muted
    }

    /// Choice on entry; -2: muted, length not known.
    readonly property int current: !page.muted ? 0 : (kept.length !== 0 ? kept.length : -2)
    /// Tapped here; -2: none yet.
    property int chosen: -2
    readonly property int shown: page.chosen !== -2 ? page.chosen : page.current
    /// Clock read once: time left as on opening.
    readonly property real now: kept.now()

    property bool saved: false
    function save() {
        if (!page.saved && page.chosen !== -2 && page.chosen !== page.current) {
            page.saved = true
            page.picked(page.chosen)
        }
    }

    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            page.save()
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column
            width: parent.width

            PageHeader {
                objectName: "pageHeader"
                //: Heading of a chat's notifications page.
                title: qsTr("Notifications")
            }

            // Silica switch self-flips; page decides.
            Repeater {
                model: Mute.choices

                TextSwitch {
                    objectName: "muteOption" + modelData
                    automaticCheck: false
                    checked: page.shown === modelData
                    text: page.current === modelData && page.chosen === -2
                          ? Mute.activeChoiceName(modelData, kept.until, page.now)
                          : Mute.choiceName(modelData)
                    onClicked: page.chosen = modelData
                }
            }

            Item {
                width: 1
                height: Theme.paddingLarge
                visible: page.mentionsStillNotify
            }

            Label {
                objectName: "mentionsNote"
                visible: page.mentionsStillNotify
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryColor
                //: Under the mute choices of a group, while "Mentions" is on
                //: in the notification settings.
                text: qsTr("You still get a notification when someone in this group replies to one of your messages.")
            }
        }
    }
}
