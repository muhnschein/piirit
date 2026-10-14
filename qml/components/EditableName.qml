import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Name under a picture: always a field, never label-then-field.
 * Centred, in the name's size and colour, with the field's underline.
 *
 * `text` is the given name. Empty field shows `fallbackText` (contact's own
 * name), else `placeholderText`, dimmed.
 *
 * `hint` shows under it only while focused. Inner items named via *ObjectName,
 * as Banner does.
 */
Item {
    id: root

    /// Field text: given name.
    property alias text: field.text
    /// Empty-field fallback: contact's own name.
    property string fallbackText: ""
    /// Empty-field placeholder, no fallback.
    property string placeholderText: ""
    /// Hint while focused.
    property string hint: ""
    /// Editable. Left group: shown, not editable.
    property bool canEdit: true

    property alias fieldObjectName: field.objectName
    property alias hintObjectName: hintLabel.objectName

    /// Has cursor. Hint and page-done check follow it.
    ///
    /// `focus`, not `activeFocus` (needs a window; tests have none).
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

        // Full width: TextField insets own text. Inset here too and the bio field
        // below misaligns.
        TextField {
            id: field
            width: parent.width
            horizontalAlignment: TextInput.AlignHCenter
            // Old label's size and colour: still a heading.
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.highlightColor
            placeholderColor: Theme.secondaryHighlightColor
            placeholderText: root.fallbackText.length > 0 ? root.fallbackText
                                                          : root.placeholderText
            readOnly: !root.canEdit
            // Silica label would sit left above centred name; hint covers it.
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
