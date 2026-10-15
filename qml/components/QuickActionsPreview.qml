import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/QuickActions.js" as QuickActions

/*
 * Small cover with one or two quick actions, as on home screen. Drawn like
 * cover/CoverPage.qml, scaled from real cover so icons match. Circles empty;
 * unchosen = dot. Reads no settings: page hands icons.
 */
BackgroundItem {
    id: preview

    /// Action slots: 1 or 2.
    property int count: 1
    /// Icon names (QuickActions.iconName), left and right; "" = not chosen.
    property string leftIcon
    property string rightIcon
    /// Reader's choice.
    property bool selected: false
    /// Choice name, under picture.
    property alias text: caption.text

    /// Real cover size: Theme's, else Jolla phone's.
    readonly property real coverWidth: Theme.coverSizeLarge
                                       && Theme.coverSizeLarge.width > 0
                                       ? Theme.coverSizeLarge.width : 234
    readonly property real coverHeight: Theme.coverSizeLarge
                                        && Theme.coverSizeLarge.height > 0
                                        ? Theme.coverSizeLarge.height : 374
    /// Scale vs real cover.
    readonly property real ratio: preview.width / preview.coverWidth

    // Cover's own measures, scaled; see CoverPage.
    readonly property int cellSize: Math.floor(picture.width / 3)
    readonly property real gap: Theme.paddingMedium * preview.ratio
    readonly property real strip: Theme.itemSizeSmall * preview.ratio
    readonly property real iconSize: Theme.iconSizeSmall * preview.ratio

    /// Circle positions `{x, y}`, cover's layout.
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

    /// Icon as whole URL; none for "".
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
                    // Fade at ring's middle (QuickActions.fade): too thin to fade across;
                    // no shader (Avatar.qml).
                    opacity: 0.6 * QuickActions.fade(y + height / 2,
                                                     faces.height - 2 * preview.strip,
                                                     faces.height)
                }
            }
        }

        // Home screen: action tops ~18% of cover height up.
        Repeater {
            model: preview.count === 2 ? [preview.leftIcon, preview.rightIcon]
                                       : [preview.leftIcon]

            Item {
                objectName: "previewAction" + index
                /// Icon; "" unchosen dot.
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
