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

    /// Topic rows, top to bottom. `key` names row (`<key>Entry`); `icon` theme's, or own drawing
    /// in qml/art/settings/ (SFOS has no flask, no cover with action).
    readonly property var topics: [
        { key: "messages", icon: "icon-m-message", title: qsTr("Messages"),
          url: "SettingsMessagesPage.qml" },
        { key: "notifications", icon: "icon-m-browser-notifications", title: qsTr("Notifications"),
          url: "SettingsNotificationsPage.qml" },
        { key: "privacy", icon: "icon-m-device-lock", title: qsTr("Privacy and security"),
          url: "SettingsPrivacyPage.qml" },
        //: Settings topic: the actions offered on the app's cover (the minimised app on the home screen).
        { key: "quickActions", icon: "../art/settings/quick-actions.svg", title: qsTr("Quick actions"),
          url: "QuickActionsPage.qml" },
        //: Settings topic: features that may still change or go away.
        { key: "experimental", icon: "../art/settings/flask.svg", title: qsTr("Experimental"),
          url: "SettingsExperimentalPage.qml" },
        { key: "about", icon: "icon-m-about", title: qsTr("About"),
          url: "AboutPage.qml" }
    ]

    /// Theme icon, or own drawing inked like one (red strokes on black, src/pictures.rs).
    function iconSource(icon, tint) {
        if (icon.indexOf("icon-") === 0) {
            return "image://theme/" + icon + "?" + tint
        }
        var ink = encodeURIComponent("" + tint)
        return "image://piirit/ink?file=" + encodeURIComponent("" + Qt.resolvedUrl(icon))
               + "&ink=" + ink + "&lit=" + ink + "&inkStrength=1&litStrength=1"
    }

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
                        objectName: modelData.key + "Icon"
                        x: Theme.horizontalPageMargin
                        anchors.verticalCenter: parent.verticalCenter
                        width: Theme.iconSizeMedium
                        height: width
                        sourceSize.width: width
                        sourceSize.height: height
                        source: page.iconSource(modelData.icon, topic.tint)
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
