import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/QuickActions.js" as QuickActions

/*
 * Cover quick actions: one or two, and what each does. Choice is two live
 * cover pictures (components/QuickActionsPreview.qml); settings below
 * (components/QuickActionSetting.qml). With one, left is used; right kept for
 * two again.
 * Own page, from settings Advanced: on settings page it crowded the rest.
 * No save: controls write on tap; cover (qml/cover/CoverPage.qml) follows.
 */
Page {
    id: page

    /// Profile a chat action's chat is picked from: the one settings page was
    /// opened from.
    property int accountId

    readonly property int actionCount: QuickActions.count(Settings)
    readonly property var leftAction: QuickActions.read(Settings, "left")
    readonly property var rightAction: QuickActions.read(Settings, "right")

    /// Icon in pictures; "" for none.
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

            // Two pictures centred, at most real cover size. Bindings, not Row:
            // see ChoiceTiles.
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
                hostPage: page
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
                hostPage: page
                accountId: page.accountId
                //: The quick action on the right of the cover.
                label: qsTr("Right")
            }
        }

        VerticalScrollDecorator {}
    }
}
