import QtQuick 2.0

// Own type so grouped `focus { ... }` binds. Modes: Camera's enums, plain
// numbers in `var` props.
QtObject {
    property var focusMode
    property var focusPointMode
    property point customFocusPoint
}
