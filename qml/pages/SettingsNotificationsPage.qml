import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * What makes the phone ring or buzz. From SettingsPage. Show notifications
 * gates content and muted-group replies (greyed, not hidden); calls ring on
 * their own switch, notifications or not.
 */
Page {
    id: page

    /// Unused: every settings page takes it (SettingsPage pushes it).
    property int accountId

    function notificationIndex(detail) {
        return detail >= 0 && detail <= 2 ? detail : 0
    }

    /// Reset choice to stored setting: Silica tap writes currentIndex, detaching binding.
    function refresh() {
        notificationCombo.currentIndex = page.notificationIndex(Settings.notificationDetail)
    }

    Connections {
        target: Settings
        onNotificationDetailChanged: page.refresh()
    }

    Component.onCompleted: page.refresh()

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Notifications")
            }

            TextSwitch {
                objectName: "notificationsSwitch"
                text: qsTr("Show notifications")
                automaticCheck: false
                checked: Settings.notificationsEnabled === true
                onClicked: Settings.notificationsEnabled = !checked
            }

            // Lock screen shows notification text: reader picks how much.
            ComboBox {
                id: notificationCombo
                objectName: "notificationCombo"
                width: parent.width
                enabled: Settings.notificationsEnabled === true
                label: qsTr("Notification content")

                menu: ContextMenu {
                    MenuItem {
                        objectName: "notificationOption0"
                        text: qsTr("Sender and message")
                        onClicked: Settings.notificationDetail = 0
                    }
                    MenuItem {
                        objectName: "notificationOption1"
                        text: qsTr("Sender only")
                        onClicked: Settings.notificationDetail = 1
                    }
                    MenuItem {
                        objectName: "notificationOption2"
                        text: qsTr("No details")
                        onClicked: Settings.notificationDetail = 2
                    }
                }
            }

            SectionHeader {
                //: Heading over whether calls ring. A plural noun.
                text: qsTr("Calls")
            }

            // From a contact; reference clients' default.
            TextSwitch {
                objectName: "callsRingSwitch"
                text: qsTr("Ring for incoming calls")
                description: qsTr("Show call screen for accepted contacts")
                automaticCheck: false
                checked: Settings.callsRing !== false
                onClicked: Settings.callsRing = !checked
            }

            SectionHeader {
                text: qsTr("Muted chats")
            }

            // Only thing through a muted group. Not reference clients' words: theirs promise
            // reactions; only quoted replies pass (`is_mention`, chatlist.rs).
            TextSwitch {
                objectName: "mentionsSwitch"
                //: A reply to one of the reader's own messages, arriving
                //: in a group they have muted.
                text: qsTr("Replies in muted groups")
                description: qsTr("Show notifications for replies to your messages in muted groups")
                enabled: Settings.notificationsEnabled === true
                automaticCheck: false
                checked: Settings.mentionNotifications === true
                onClicked: Settings.mentionNotifications = !checked
            }
        }

        VerticalScrollDecorator {}
    }
}
