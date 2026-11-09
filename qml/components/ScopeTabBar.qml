import QtQuick 2.6
import Sailfish.Silica 1.0

/*
 * Tab strip, active tab underlined: Clock and Settings apps' top strip. From Vuo
 * (same author, GPL-3.0), rebuilt from public API: Silica's TabBar is in
 * Sailfish.Silica.private (Harbour refuses). Host owns `currentIndex`.
 */
Item {
    id: root

    /// Translated words only.
    property var titles: []
    property int currentIndex: 0
    /// Host page, for orientation; may be null (tests load file alone).
    property Item hostPage: null

    signal tabClicked(int index)

    property bool _portrait: root.hostPage ? root.hostPage.isPortrait : true
    // `row.children` read: binding re-runs as Repeater fills (`itemAt` alone doesn't notify).
    property Item _currentButton: root.currentIndex >= 0 && root.currentIndex < tabs.count
                                  ? (row.children, tabs.itemAt(root.currentIndex))
                                  : null
    // Stands where PageHeader would: clears camera cutout as header does, portrait only.
    property real _topMargin: root._portrait && Screen.topCutout.height > 0
                              ? Math.max(0, Screen.topCutout.height - Theme.paddingLarge)
                              : 0
    // Underline snaps on first layout, slides once moved.
    property bool _animate: false

    implicitHeight: root._topMargin + row.height

    onCurrentIndexChanged: root._animate = true

    Flickable {
        id: strip

        y: root._topMargin
        width: root.width
        height: row.height
        contentWidth: row.width
        boundsBehavior: Flickable.StopAtBounds
        interactive: strip.contentWidth > strip.width
        // Active tab centred when strip wider than screen.
        contentX: {
            var button = root._currentButton
            if (!button) {
                return 0
            }
            return Math.max(0, Math.min(strip.contentWidth - strip.width,
                                        button.x + (button.width - strip.width) / 2))
        }

        Row {
            id: row

            // Slack split to outer tabs: strip centred. Reads `tabContentWidth`: no loop.
            property real extraMargin: {
                var total = 0
                for (var i = 0; i < row.children.length; ++i) {
                    total += row.children[i].tabContentWidth || 0
                }
                return Math.max(0, strip.width - total) / 2
            }

            // One size down when words don't fit. Metrics, not widths read back: loop.
            property int titleFontSize: {
                var total = 0
                for (var i = 0; i < root.titles.length; ++i) {
                    total += largeMetrics.advanceWidth(root.titles[i]) + Theme.paddingLarge * 2
                }
                return total > root.width ? Theme.fontSizeMedium : Theme.fontSizeLarge
            }

            Repeater {
                id: tabs
                model: root.titles.length

                ScopeTabButton {
                    objectName: "tab" + index
                    title: root.titles[index]
                    tabIndex: index
                    tabCount: tabs.count
                    current: index === root.currentIndex
                    portrait: root._portrait
                    titleFontSize: row.titleFontSize
                    extraMargin: row.extraMargin
                    onClicked: root.tabClicked(index)
                }
            }
        }

        // Highlight rule as wide as active word; Silica's 200 ms slide.
        Rectangle {
            id: underline
            objectName: "tabUnderline"

            property Item _label: root._currentButton ? root._currentButton.labelItem : null

            x: underline._label ? root._currentButton.x + underline._label.x : 0
            y: underline._label
               ? underline._label.y + underline._label.height + Theme.paddingMedium
               : 0
            width: underline._label ? underline._label.width : 0
            height: Theme._lineWidth
            color: Theme.highlightColor

            Behavior on x {
                enabled: root._animate
                SmoothedAnimation { duration: 200; easing.type: Easing.InOutQuad }
            }
            Behavior on width {
                enabled: root._animate
                SmoothedAnimation { duration: 200; easing.type: Easing.InOutQuad }
            }
        }
    }

    FontMetrics {
        id: largeMetrics
        font.pixelSize: Theme.fontSizeLarge
    }
}
