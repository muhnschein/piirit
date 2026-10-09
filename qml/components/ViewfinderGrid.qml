import QtQuick 2.0

/*
 * The rule of thirds over the viewfinder: two lines each way, thin and
 * pale, as the platform camera draws its grid. Plain rectangles: the
 * platform's own line is a private Silica type.
 */
Item {
    id: grid

    Repeater {
        model: 2
        Rectangle {
            x: Math.round(grid.width * (index + 1) / 3)
            width: 1
            height: grid.height
            color: Qt.rgba(1, 1, 1, 0.5)
        }
    }

    Repeater {
        model: 2
        Rectangle {
            y: Math.round(grid.height * (index + 1) / 3)
            width: grid.width
            height: 1
            color: Qt.rgba(1, 1, 1, 0.5)
        }
    }
}
