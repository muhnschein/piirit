import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/QuickActions.js" as QuickActions

/*
 * The cover's quick actions: a line saying what they are, then the choice
 * between one and two, then what each does.
 *
 * The choice is two pictures of the cover side by side
 * (components/QuickActionsPreview.qml), one with room for one action and
 * one with room for two, each with the actions as they are set up now:
 * the reader sees what each would look like before tapping it, and sees
 * it change as the actions under it change. Under them, what the chosen
 * one needs -- the action, or the left one and the right
 * (components/QuickActionSetting.qml). With one, the left action is the
 * one; what was set up on the right is kept for when there are two
 * again.
 *
 * A page of its own, reached from the settings page's Advanced section:
 * a chat's action brings a chat and a row of icons with it, and on the
 * settings page itself the two sides crowded out everything around them.
 *
 * Nothing here needs saving: each control writes its setting on the tap,
 * and the cover (qml/cover/CoverPage.qml) follows the setting.
 */
Page {
    id: page

    /// Whose chats a chat's action is picked from: the profile the
    /// settings page was pulled down from.
    property int accountId

    readonly property int actionCount: QuickActions.count(Settings)
    readonly property var leftAction: QuickActions.read(Settings, "left")
    readonly property var rightAction: QuickActions.read(Settings, "right")

    /// The icon an action wears in the pictures, "" for none yet.
    function iconOf(action) {
        return action.kind === "" ? "" : QuickActions.iconName(action)
    }

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

            // The two pictures, side by side and centred, as big as a
            // real cover at most. Laid out by bindings rather than a Row,
            // for the reason ChoiceTiles gives.
            Item {
                id: choices
                objectName: "quickActionCountChoices"
                width: parent.width
                height: Math.max(one.height, two.height) + Theme.paddingMedium

                readonly property real between: Theme.paddingLarge * 2
                readonly property real tileWidth: Math.min(
                    (choices.width - 2 * Theme.horizontalPageMargin - choices.between) / 2,
                    choices.width * 0.36, one.coverWidth)

                QuickActionsPreview {
                    id: one
                    objectName: "oneActionPreview"
                    x: choices.width / 2 - choices.between / 2 - width
                    width: choices.tileWidth
                    count: 1
                    leftIcon: page.iconOf(page.leftAction)
                    selected: page.actionCount === 1
                    //: Under a picture of the cover with room for one
                    //: quick action; tapping it chooses that.
                    text: qsTr("One action")
                    onClicked: Settings.quickActionCount = 1
                }

                QuickActionsPreview {
                    id: two
                    objectName: "twoActionsPreview"
                    x: choices.width / 2 + choices.between / 2
                    width: choices.tileWidth
                    count: 2
                    leftIcon: page.iconOf(page.leftAction)
                    rightIcon: page.iconOf(page.rightAction)
                    selected: page.actionCount === 2
                    //: Under a picture of the cover with room for two
                    //: quick actions; tapping it chooses that.
                    text: qsTr("Two actions")
                    onClicked: Settings.quickActionCount = 2
                }
            }

            QuickActionSetting {
                objectName: "leftQuickAction"
                side: "left"
                accountId: page.accountId
                label: page.actionCount === 2
                       //: The quick action on the left of the cover.
                       ? qsTr("Left")
                       //: The one quick action on the cover, when it has
                       //: room for one.
                       : qsTr("Action")
            }

            QuickActionSetting {
                objectName: "rightQuickAction"
                visible: page.actionCount === 2
                side: "right"
                accountId: page.accountId
                //: The quick action on the right of the cover.
                label: qsTr("Right")
            }
        }

        VerticalScrollDecorator {}
    }
}
