import QtQuick 2.6
import Sailfish.Silica 1.0

/*
 * One tab of ScopeTabBar; Silica TabButton's metrics, public API only. Underline
 * follows word (`labelItem`), not button.
 */
BackgroundItem {
    id: button

    /// Translated word only.
    property string title
    property bool current: false
    property bool portrait: true
    property int titleFontSize: Theme.fontSizeLarge
    /// Half strip slack, on first and last tab.
    property real extraMargin: 0
    property int tabIndex: 0
    property int tabCount: 1

    property Item labelItem: label

    /// Width without `extraMargin`: strip sums these for it.
    property real tabContentWidth: 2 * Theme.paddingLarge + label.implicitWidth

    width: button.tabContentWidth
           + (button.tabIndex === 0 ? button.extraMargin : 0)
           + (button.tabIndex === button.tabCount - 1 ? button.extraMargin : 0)
    // Tall upright, short on side; words keep size.
    height: Math.max(button.portrait ? Theme.itemSizeLarge : Theme.itemSizeSmall,
                     label.implicitHeight
                     + 2 * (button.portrait ? Theme.paddingLarge : Theme.paddingMedium))

    Label {
        id: label

        // Outer tabs hug inner edge: slack stays outside word.
        x: {
            if (button.tabCount > 1 && button.tabIndex === 0) {
                return button.width - label.width - Theme.paddingMedium
            }
            if (button.tabCount > 1 && button.tabIndex === button.tabCount - 1) {
                return Theme.paddingMedium
            }
            return (button.width - label.width) / 2
        }
        y: (button.height - label.height) / 2

        text: button.title
        textFormat: Text.PlainText
        font.pixelSize: button.titleFontSize
        color: button.highlighted || button.current ? Theme.highlightColor
                                                    : Theme.primaryColor
    }
}
