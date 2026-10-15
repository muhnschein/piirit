import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Name under picture: always a field, centred, name size and colour, with
 * underline. Empty shows `fallbackText`, else `placeholderText`, dimmed.
 * `hint` only while focused. Inner items named via *ObjectName (as Banner).
 */
Item {
    id: root

    /// Given name.
    property alias text: field.text
    /// Empty fallback: contact's own name.
    property string fallbackText: ""
    /// Placeholder without fallback.
    property string placeholderText: ""
    /// Hint while focused.
    property string hint: ""
    /// Editable (left group: not).
    property bool canEdit: true

    property alias fieldObjectName: field.objectName
    property alias hintObjectName: hintLabel.objectName

    /// Has cursor; hint and page-done follow. `focus`: `activeFocus` needs a
    /// window, tests have none.
    readonly property bool editing: field.focus

    width: parent ? parent.width : 0
    height: column.height

    /// Focus field, for page opening empty.
    function edit() {
        field.forceActiveFocus()
    }

    /// Unfocus on page exit, closing keyboard.
    function done() {
        field.focus = false
    }

    Column {
        id: column
        width: parent.width

        // Full width: TextField insets text itself; more misaligns bio below.
        TextField {
            id: field
            width: parent.width
            horizontalAlignment: TextInput.AlignHCenter
            // Heading size and colour.
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.highlightColor
            placeholderColor: Theme.secondaryHighlightColor
            placeholderText: root.fallbackText.length > 0 ? root.fallbackText
                                                          : root.placeholderText
            readOnly: !root.canEdit
            // Silica label sits left of centred name; hint replaces it.
            labelVisible: false
        }

        Label {
            id: hintLabel
            visible: root.hint.length > 0 && root.editing
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: root.hint
        }
    }
}
