import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Per-phone settings, from chat list: one row per topic, each its own page (as
 * salama). Nothing here changes a setting. Profile first: its page holds name,
 * picture, relays.
 *
 * dconf via `Settings` singleton (qml/components/Settings.qml). Not a system
 * Settings page: entry outside Harbour-allowed paths, needs more besides.
 */
Page {
    id: page

    /// Profile the chat list is on: profile row, block list.
    property int accountId

    /// Topic rows, top to bottom. `key` names row (`<key>Entry`).
    readonly property var topics: [
        { key: "messages", icon: "icon-m-message", title: qsTr("Messages"),
          url: "SettingsMessagesPage.qml" },
        { key: "notifications", icon: "icon-m-notifications", title: qsTr("Notifications"),
          url: "SettingsNotificationsPage.qml" },
        { key: "privacy", icon: "icon-m-device-lock", title: qsTr("Privacy and security"),
          url: "SettingsPrivacyPage.qml" },
        //: Settings topic: what the app shows on its cover (the minimised app on the home screen).
        { key: "cover", icon: "icon-m-device", title: qsTr("Cover"),
          url: "QuickActionsPage.qml" },
        //: Settings topic: features that may still change or go away.
        { key: "experimental", icon: "icon-m-developer-mode", title: qsTr("Experimental"),
          url: "SettingsExperimentalPage.qml" },
        { key: "about", icon: "icon-m-about", title: qsTr("About"),
          url: "AboutPage.qml" }
    ]

    function open(url) {
        pageStack.push(Qt.resolvedUrl(url), { accountId: page.accountId })
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width

            PageHeader {
                title: qsTr("Settings")
            }

            // Core's list holds every profile; only chat list's shows.
            Repeater {
                model: core.account_list

                BackgroundItem {
                    objectName: "profileEntry"
                    visible: model.account_id === page.accountId
                    width: column.width
                    height: visible ? profileRow.height : 0

                    ContactRow {
                        id: profileRow
                        width: parent.width
                        displayName: model.display_name.length > 0
                                     ? model.display_name : model.addr
                        //: Under the reader's own name at the top of the settings.
                        address: qsTr("Your profile")
                        showAddress: true
                        ownColor: model.color
                        picturePath: model.avatar_path
                    }

                    onClicked: page.open("ProfilePage.qml")
                }
            }

            Repeater {
                model: page.topics

                BackgroundItem {
                    id: topic
                    objectName: modelData.key + "Entry"
                    width: column.width
                    height: Theme.itemSizeSmall

                    readonly property color tint: topic.highlighted ? Theme.highlightColor
                                                                     : Theme.primaryColor

                    Image {
                        id: topicIcon
                        x: Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        width: Theme.iconSizeMedium
                        height: width
                        source: "image://theme/" + modelData.icon + "?" + topic.tint
                    }

                    Label {
                        objectName: modelData.key + "Label"
                        x: topicIcon.x + topicIcon.width + Theme.paddingMedium
                        width: parent.width - x - Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        truncationMode: TruncationMode.Fade
                        color: topic.tint
                        text: modelData.title
                    }

                    onClicked: page.open(modelData.url)
                }
            }
        }

        VerticalScrollDecorator {}
    }
}
