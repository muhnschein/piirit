import QtQuick 2.0

// The camera's focus settings, a type of its own so the page's grouped
// `focus { ... }` assignment resolves against declared properties. The
// modes are Camera's enums, held as plain numbers in the `var`
// properties.
QtObject {
    property var focusMode
    property var focusPointMode
    property point customFocusPoint
}
