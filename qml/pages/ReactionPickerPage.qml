import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Emoji.js" as Emoji

/*
 * Any emoji as a reaction: what the "+" at the end of a message's quick
 * reactions opens.
 *
 * One tap reacts and goes back to the conversation; going back by the
 * edge picks nothing. Reports the pick with a signal and leaves sending
 * it to the page that asked, the way ChatPickerPage does: the message
 * row this was opened from may be gone by the time the answer comes.
 *
 * The emoji are drawn as pictures (EmojiGlyph): Sailfish can draw only a
 * handful itself. The list is Unicode's, without the skin-tone variants,
 * in its groups, with what the reader picked here recently first.
 *
 * Laid out by bindings rather than positioners, as the rest of the tree
 * is, so a test can load it and find every emoji where it says it is.
 */
Page {
    id: page

    /// The reader's own reaction on the message, ringed in the grid;
    /// "" for none. Tapping it takes it off, as tapping its chip does.
    property string current: ""
    /// What the reader picked here before, newest first.
    property var recent: []

    /// The reader picked this emoji. `current` itself when the ringed
    /// one was tapped, spelled exactly as it was sent, so the core takes
    /// it as the same reaction and takes it off.
    signal picked(string emoji)

    /// What the search field holds, once the reader stops typing.
    property string query: ""

    /// The titles of Unicode's groups, in EmojiData.js's numbering.
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

    /// The recent ones' group: before every group in the data.
    readonly property int recentGroup: -1

    /// How many emoji fit across. Square cells a little bigger than a
    /// fingertip, however wide the page is turned.
    readonly property int columns: Math.max(5, Math.floor(width / Theme.itemSizeSmall))
    readonly property real cellSize: width / columns

    /// What a search finds; [] while there is no search.
    readonly property var results: page.query.length > 0 ? Emoji.search(page.query) : []

    /// What the grid shows, a row at a time: a search's results alone,
    /// or the recent ones and then every group under its heading.
    readonly property var rows: {
        if (page.query.length > 0) {
            return Emoji.rows(page.results, page.columns, null)
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
            out = out.concat(Emoji.rows(shown, page.columns, null))
        }
        return out.concat(Emoji.rows(Emoji.all(), page.columns, page.groupTitles))
    }

    /// The group whose rows are at the top of the grid now, for the
    /// jump strip to light.
    property int topGroup: page.recentGroup

    function choose(emoji) {
        var mine = page.current.length > 0 && Emoji.key(emoji) === Emoji.key(page.current)
        page.picked(mine ? page.current : emoji)
        pageStack.pop()
    }

    /// Scroll the grid to a group's heading.
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

        // One button per group, to jump there; gone while searching,
        // when there are no groups to jump between.
        Item {
            id: strip
            objectName: "groupStrip"
            y: header.height + searchField.height
            width: parent.width
            height: page.query.length > 0 ? 0 : Theme.itemSizeExtraSmall
            visible: height > 0

            Repeater {
                model: Emoji.groupIcons

                BackgroundItem {
                    objectName: "groupButton"
                    readonly property int group: index
                    x: index * width
                    width: strip.width / Emoji.groupIcons.length
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
        // Under the search field, not in a header: rows flicked up would
        // otherwise draw over it.
        clip: true
        model: page.rows
        onContentYChanged: page.followScroll()

        delegate: Item {
            id: row
            objectName: "emojiRow"
            readonly property var entry: modelData
            width: grid.width
            height: entry.heading.length > 0 ? sectionTitle.height : page.cellSize

            SectionHeader {
                id: sectionTitle
                visible: row.entry.heading.length > 0
                text: row.entry.heading
            }

            Repeater {
                model: row.entry.cells

                BackgroundItem {
                    objectName: "emojiOption"
                    readonly property string emoji: modelData[0]
                    readonly property bool mine: page.current.length > 0
                                                 && Emoji.key(emoji) === Emoji.key(page.current)
                    x: index * page.cellSize
                    width: page.cellSize
                    height: page.cellSize
                    onClicked: page.choose(emoji)

                    // The reader's own reaction, ringed: tapping it takes
                    // it off.
                    Rectangle {
                        objectName: "currentRing"
                        visible: parent.mine
                        anchors.centerIn: parent
                        width: parent.width - Theme.paddingSmall
                        height: width
                        radius: width / 2
                        color: Theme.rgba(Theme.highlightBackgroundColor,
                                          Theme.highlightBackgroundOpacity)
                        border.color: Theme.highlightColor
                        border.width: Math.max(1, Math.round(Theme.paddingSmall / 2))
                    }

                    EmojiGlyph {
                        anchors.centerIn: parent
                        size: Math.round(page.cellSize * 0.6)
                        emoji: modelData[0]
                    }
                }
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
