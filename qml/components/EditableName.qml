import QtQuick 2.0
import Sailfish.Silica 1.0
import "../js/Emoji.js" as Emoji

/*
 * Name under picture: always a field, centred, name size and colour, with
 * underline. Empty shows `fallbackText`, else `placeholderText`, dimmed.
 * `hint` only while focused. Inner items named via *ObjectName (as Banner).
 * Field can't draw pictures: name with emoji, not being edited, drawn by label
 * over it (Twemoji, Emoji.js), field's own text see-through.
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

    /// Name or fallback with emoji pictures, while not editing; "" when field draws it.
    readonly property string emojiName: root.editing ? ""
        : Emoji.inText(field.text.length > 0 ? field.text : field.placeholderText, false,
                       field.font.pixelSize, Emoji.nameTint(Theme.highlightColor))

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
            color: root.emojiName.length > 0 ? "transparent" : Theme.highlightColor
            placeholderColor: root.emojiName.length > 0 ? "transparent"
                                                        : Theme.secondaryHighlightColor
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

    // Where field draws its text: between its margins, about its text's centre.
    Label {
        objectName: "emojiName"
        visible: root.emojiName.length > 0
        x: field.textLeftMargin
        y: field.textVerticalCenterOffset - height / 2
        width: field.width - field.textLeftMargin - field.textRightMargin
        horizontalAlignment: Text.AlignHCenter
        truncationMode: TruncationMode.Fade
        font.pixelSize: field.font.pixelSize
        color: field.text.length > 0 ? Theme.highlightColor : Theme.secondaryHighlightColor
        textFormat: Text.StyledText
        text: root.emojiName
    }
}
