import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/QuickActions.js" as QuickActions

/*
 * A cover, drawn small, with room for one quick action or for two: one
 * of the two choices on the quick actions' page (pages/
 * QuickActionsPage.qml), and a picture of what that choice puts on the
 * home screen.
 *
 * Drawn the way the cover draws itself (cover/CoverPage.qml): the same
 * staggered grid, three circles across with every other row shifted half
 * a circle and cut off at both edges and the first row seen from the
 * waist down; the same fade into the strip along the bottom edge, from
 * the same shader; and the actions' own icons, from the files the cover
 * hands the home screen, where the home screen puts them -- one in the
 * middle, or one in the middle of each half. Everything is scaled from a
 * real cover's size, so the icons are as big against the faces as they
 * will be. The circles are empty: this is a picture of where things go
 * rather than of whoever the reader talks to. An action not chosen yet
 * is a dot in its place.
 *
 * It reads no settings itself: the page hands it the icons, so it is
 * redrawn as the choices under it change.
 */
BackgroundItem {
    id: preview

    /// How many actions this cover has room for: 1 or 2.
    property int count: 1
    /// The icons the actions wear, by name (QuickActions.iconName), left
    /// and right; "" for an action not chosen yet. With room for one,
    /// only the left is read.
    property string leftIcon
    property string rightIcon
    /// Whether this is the choice the reader has made.
    property bool selected: false
    /// What the choice is called, under the picture.
    property alias text: caption.text

    /// The size of a real cover, which the picture is scaled from. Theme
    /// says; a Theme that does not is taken to mean the Jolla phone's.
    readonly property real coverWidth: Theme.coverSizeLarge
                                       && Theme.coverSizeLarge.width > 0
                                       ? Theme.coverSizeLarge.width : 234
    readonly property real coverHeight: Theme.coverSizeLarge
                                        && Theme.coverSizeLarge.height > 0
                                        ? Theme.coverSizeLarge.height : 374
    /// How much smaller than a real cover the picture is.
    readonly property real ratio: preview.width / preview.coverWidth

    // The cover's own measures, scaled; see CoverPage for each.
    readonly property int cellSize: Math.floor(picture.width / 3)
    readonly property real gap: Theme.paddingMedium * preview.ratio
    readonly property real strip: Theme.itemSizeSmall * preview.ratio
    readonly property real iconSize: Theme.iconSizeSmall * preview.ratio

    /// Where the circles go: `{x, y}` for each, laid out the cover's way.
    readonly property var cells: {
        var list = []
        var size = preview.cellSize
        if (size <= 0) {
            return list
        }
        var step = Math.max(1, Math.round(size * 0.95))
        var cut = Math.round(size * 0.6)
        var rows = Math.ceil((picture.height + cut) / step)
        for (var row = 0; row < rows; row++) {
            var shifted = row % 2 === 1
            for (var col = 0; col < (shifted ? 4 : 3); col++) {
                list.push({
                    x: col * size - (shifted ? size / 2 : 0),
                    y: row * step - cut
                })
            }
        }
        return list
    }

    /// The picture for an icon, as a whole URL; none for "".
    function iconSource(name) {
        if (name === "") {
            return ""
        }
        return Qt.resolvedUrl("../" + QuickActions.iconFile(
            name, preview.iconSize, QuickActions.isLight(Theme.primaryColor)))
    }

    height: picture.height + Theme.paddingSmall + caption.height

    Rectangle {
        id: picture
        objectName: "previewCover"
        width: preview.width
        height: Math.round(preview.width * preview.coverHeight / preview.coverWidth)
        radius: Theme.paddingMedium
        color: Theme.rgba(Theme.highlightDimmerColor, 0.6)
        border.width: preview.selected ? Math.max(2, Math.round(Theme.paddingSmall / 2)) : 0
        border.color: Theme.highlightColor

        Item {
            id: faces
            anchors.fill: parent
            clip: true

            layer.enabled: true
            layer.effect: ShaderEffect {
                property real fadeFrom: faces.height - 2 * preview.strip
                property real fadeTo: faces.height
                property real gridHeight: Math.max(1, faces.height)
                fragmentShader: QuickActions.fadeShader
            }

            Repeater {
                model: preview.cells

                Rectangle {
                    x: modelData.x + preview.gap / 2
                    y: modelData.y + preview.gap / 2
                    width: preview.cellSize - preview.gap
                    height: width
                    radius: width / 2
                    color: "transparent"
                    border.width: Math.max(1, Math.round(2 * preview.ratio))
                    border.color: Theme.secondaryColor
                    opacity: 0.6
                }
            }
        }

        // Where the home screen draws the actions: their tops about 18%
        // of the cover's height up, as the cover measures it.
        Repeater {
            model: preview.count === 2 ? [preview.leftIcon, preview.rightIcon]
                                       : [preview.leftIcon]

            Item {
                objectName: "previewAction" + index
                /// The icon's picture, "" for the dot of one not chosen.
                readonly property string source: preview.iconSource(modelData)
                width: preview.iconSize
                height: preview.iconSize
                x: picture.width * (preview.count === 2 ? (index === 0 ? 0.25 : 0.75) : 0.5)
                   - width / 2
                y: Math.round(picture.height * 0.82)

                Image {
                    anchors.fill: parent
                    visible: parent.source !== ""
                    source: parent.source
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                }

                Rectangle {
                    anchors.centerIn: parent
                    visible: parent.source === ""
                    width: Math.round(parent.width * 0.4)
                    height: width
                    radius: width / 2
                    color: Theme.secondaryColor
                    opacity: 0.6
                }
            }
        }
    }

    Label {
        id: caption
        y: picture.height + Theme.paddingSmall
        width: preview.width
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        font.pixelSize: Theme.fontSizeSmall
        color: preview.selected || preview.highlighted ? Theme.highlightColor
                                                      : Theme.primaryColor
    }
}
