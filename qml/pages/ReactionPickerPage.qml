import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Emoji.js" as Emoji
import "../js/EmojiPicker.js" as Picker

/*
 * Any emoji as reaction ("+" after quick reactions). Tap reacts, returns; back picks
 * none. Asking page sends (as ChatPickerPage): source row may be gone. Pictures
 * (EmojiGlyph: Sailfish draws few), text if reader wants system's. Unicode minus
 * skin tones, by group, recent first. Bindings, not positioners: tests find each.
 */
Page {
    id: page

    /// Own reaction, marked; "" none.
    property string current: ""
    /// Recently picked here, newest first.
    property var recent: []

    /// Emoji picked; marked one tapped gives `current` spelled exactly: core removes it.
    signal picked(string emoji)

    /// Search text, once typing stops.
    property string query: ""

    /// Group titles, EmojiData.js numbering.
    readonly property var groupTitles: [
        qsTr("Smileys & emotion"),
        qsTr("People & body"),
        qsTr("Animals & nature"),
        qsTr("Food & drink"),
        qsTr("Travel & places"),
        qsTr("Activities"),
        qsTr("Objects"),
        qsTr("Symbols"),
        qsTr("Flags")
    ]

    /// Recent group: first.
    readonly property int recentGroup: -1

    /// Emoji per row: roomy square cells, any width (packed grid hard to aim).
    readonly property int columns: Math.max(5, Math.floor(width / Theme.itemSizeMedium))
    readonly property real cellSize: width / columns
    /// Emoji size in its cell.
    readonly property real glyphSize: Math.round(page.cellSize * 0.5)
    /// `current` as grid compares it, once.
    readonly property string currentKey: page.current.length > 0 ? Emoji.key(page.current) : ""

    /// Search results; [] when no search.
    readonly property var results: page.query.length > 0 ? Picker.search(page.query) : []

    /// Grid rows: search results, or recent then each group.
    readonly property var rows: {
        if (page.query.length > 0) {
            return Picker.rows(page.results, page.columns, null)
        }
        var out = []
        var shown = []
        for (var i = 0; i < page.recent.length && shown.length < 2 * page.columns; i++) {
            var picture = Emoji.pictureFor(page.recent[i])
            if (picture.length > 0) {
                shown.push([page.recent[i], picture, page.recentGroup])
            }
        }
        if (shown.length > 0) {
            out.push({ heading: qsTr("Recent"), group: page.recentGroup, cells: [] })
            out = out.concat(Picker.rows(shown, page.columns, null))
        }
        return out.concat(Picker.rows(Picker.all(), page.columns, page.groupTitles))
    }

    /// Top group, lit on jump strip.
    property int topGroup: page.recentGroup

    function choose(emoji) {
        var mine = page.currentKey.length > 0 && Emoji.key(emoji) === page.currentKey
        page.picked(mine ? page.current : emoji)
        pageStack.pop()
    }

    /// Scroll grid to group heading.
    function jumpTo(group) {
        for (var i = 0; i < page.rows.length; i++) {
            if (page.rows[i].heading.length > 0 && page.rows[i].group === group) {
                grid.positionViewAtIndex(i, ListView.Beginning)
                page.topGroup = group
                return
            }
        }
    }

    function followScroll() {
        var index = grid.indexAt(0, grid.contentY + 1)
        if (index >= 0 && index < page.rows.length) {
            page.topGroup = page.rows[index].group
        }
    }

    Timer {
        id: searchDebounce
        interval: 200
        onTriggered: page.query = searchField.text.trim()
    }

    Item {
        id: heading
        width: parent.width
        height: header.height + searchField.height + strip.height

        PageHeader {
            id: header
            width: parent.width
            title: qsTr("React")
        }

        SearchField {
            id: searchField
            objectName: "reactionSearchField"
            y: header.height
            width: parent.width
            placeholderText: qsTr("Search")
            onTextChanged: searchDebounce.restart()
        }

        // Jump per group; hidden while searching.
        Item {
            id: strip
            objectName: "groupStrip"
            y: header.height + searchField.height
            width: parent.width
            height: page.query.length > 0 ? 0 : Theme.itemSizeExtraSmall
            visible: height > 0

            Repeater {
                model: Picker.groupIcons

                BackgroundItem {
                    objectName: "groupButton"
                    readonly property int group: index
                    x: index * width
                    width: strip.width / Picker.groupIcons.length
                    height: strip.height
                    highlighted: down || page.topGroup === index
                    onClicked: page.jumpTo(index)

                    EmojiGlyph {
                        anchors.centerIn: parent
                        size: Math.min(parent.width, parent.height) * 0.55
                        opacity: parent.highlighted ? 1.0 : 0.6
                        emoji: modelData
                    }
                }
            }
        }
    }

    SilicaListView {
        id: grid
        objectName: "emojiGrid"
        anchors {
            top: heading.bottom
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        // Not in header: flicked rows drew over field.
        clip: true
        // Count, not rows: JS list given to view is copied whole, then per delegate;
        // scrolling stalled. Delegates read own row.
        model: page.rows.length
        onContentYChanged: page.followScroll()

        delegate: Item {
            id: row
            objectName: "emojiRow"
            readonly property var entry: page.rows[index]
            width: grid.width
            height: entry.heading.length > 0 ? sectionTitle.height : page.cellSize
            /// Pressed cell: row's one highlight.
            property int pressedCell: -1
            /// Cell with reader's own reaction, or -1.
            readonly property int markedCell: {
                if (page.currentKey.length === 0) {
                    return -1
                }
                for (var i = 0; i < entry.cells.length; i++) {
                    if (Emoji.key(entry.cells[i][0]) === page.currentKey) {
                        return i
                    }
                }
                return -1
            }

            // Heading row only. Loader unsized: it would pass width to header,
            // whose inset x pushed text off screen; header sizes from row.
            Loader {
                id: sectionTitle
                active: row.entry.heading.length > 0
                sourceComponent: SectionHeader {
                    objectName: "emojiSection"
                    width: row.width - 2 * Theme.horizontalPageMargin
                    text: row.entry.heading
                }
            }

            // Press feedback per row, not cell: build cost is flick cost.
            Rectangle {
                visible: row.pressedCell >= 0
                x: row.pressedCell * page.cellSize
                width: page.cellSize
                height: page.cellSize
                color: Theme.rgba(Theme.highlightBackgroundColor,
                                  Theme.highlightBackgroundOpacity)
            }

            // Plain touch cells: BackgroundItems were most of row weight.
            Repeater {
                model: row.entry.cells.length

                MouseArea {
                    id: cell
                    objectName: "emojiOption"
                    readonly property string emoji: row.entry.cells[index][0]
                    readonly property bool mine: index === row.markedCell
                    x: index * page.cellSize
                    width: page.cellSize
                    height: page.cellSize
                    onPressedChanged: row.pressedCell = pressed ? index : -1
                    function choose() {
                        page.choose(cell.emoji)
                    }
                    onClicked: choose()

                    EmojiGlyph {
                        anchors.centerIn: parent
                        size: page.glyphSize
                        emoji: cell.emoji
                    }
                }
            }

            // Own reaction underlined; tap removes. One per row.
            Rectangle {
                objectName: "currentMark"
                visible: row.markedCell >= 0
                x: row.markedCell * page.cellSize + (page.cellSize - width) / 2
                y: (page.cellSize + page.glyphSize) / 2 + Theme.paddingSmall
                width: page.glyphSize
                height: Math.max(2, Math.round(Theme.paddingSmall / 2))
                radius: height / 2
                color: Theme.highlightColor
            }
        }

        ViewPlaceholder {
            objectName: "noEmojiPlaceholder"
            enabled: page.query.length > 0 && page.results.length === 0
            text: qsTr("No emoji found")
            hintText: qsTr("Emoji names are in English, like \"heart\"")
        }

        VerticalScrollDecorator {}
    }
}
