import QtQuick 2.0

// Silica swipe pager. Repeater builds every delegate at once (wrong geometry, right
// bindings); `moveTo` jumps.
Item {
    id: root

    property var model: 0
    property Component delegate: null
    property int currentIndex: 0
    readonly property int count: repeater.count
    readonly property Item currentItem: repeater.count > currentIndex && currentIndex >= 0
                                        ? repeater.itemAt(currentIndex)
                                        : null
    property bool interactive: true
    readonly property bool dragging: false
    readonly property bool moving: false
    property int cacheSize: 0
    property real horizontalSpacing: 0

    function moveTo(index) { root.currentIndex = index }
    function itemAt(index) { return repeater.itemAt(index) }

    Repeater {
        id: repeater
        model: root.model
        delegate: root.delegate
    }
}
