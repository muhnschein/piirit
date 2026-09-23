import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * The cover's quick actions: a line saying what they are, then the left
 * one and the right (components/QuickActionSetting.qml), each what it
 * does and, for a chat, which chat and which icon it wears.
 *
 * A page of its own, reached from the settings page, where each side is
 * one row saying what it does now: a chat's action brings a chat and a
 * row of icons with it, and on the settings page itself the two sides
 * crowded out everything around them.
 *
 * Nothing here needs saving: each control writes its setting on the tap,
 * and the cover (qml/cover/CoverPage.qml) follows the setting.
 */
Page {
    id: page

    /// Whose chats a chat's action is picked from: the profile the
    /// settings page was pulled down from.
    property int accountId

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                objectName: "quickActionsHeader"
                title: qsTr("Quick actions")
            }

            Label {
                objectName: "quickActionsExplained"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                //: The cover is the app's thumbnail on the Sailfish home
                //: screen while the app runs in the background. Use the
                //: same word for "quick actions" as this page's title.
                text: qsTr("Piirit's cover on the home screen can show up to two quick actions.")
            }

            QuickActionSetting {
                objectName: "leftQuickAction"
                side: "left"
                accountId: page.accountId
                //: The quick action on the left of the cover.
                label: qsTr("Left")
            }

            QuickActionSetting {
                objectName: "rightQuickAction"
                side: "right"
                accountId: page.accountId
                //: The quick action on the right of the cover.
                label: qsTr("Right")
            }
        }

        VerticalScrollDecorator {}
    }
}
