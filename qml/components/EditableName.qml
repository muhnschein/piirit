import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * A name under a picture: a field, always.
 *
 * A field rather than a label that turns into one on a tap: on a page
 * whose whole purpose is the name, a second state and a badge to find
 * buy nothing. Centred under the picture, in the size and colour the
 * name is drawn in, with the line under it a field has.
 *
 * `text` is what was given here, which is what the page reads and
 * writes. An empty field shows `fallbackText` -- the name a contact
 * chose for themselves -- or the placeholder when there is no such
 * thing, both dimmed, as a field shows a placeholder. So a blank field
 * still says what the name will be.
 *
 * `hint` sits under it while the field has the cursor and not otherwise:
 * what the field means is worth a line to somebody about to type, and
 * clutter to everybody else.
 *
 * Used for a contact's name on their page, the reader's own on the
 * profile page, and a group's on both of its. Each names the inner items
 * for its tests through the *ObjectName properties, the way Banner does.
 */
Item {
    id: root

    /// The field's text: the name given here.
    property alias text: field.text
    /// Shown in the empty field. For a contact, the name they chose for
    /// themselves.
    property string fallbackText: ""
    /// Shown in the empty field when there is no fallback either.
    property string placeholderText: ""
    /// Said under the field while it has the cursor.
    property string hint: ""
    /// Whether the name can be changed at all. A group this account has
    /// left keeps its name on the screen and takes no edits.
    property bool canEdit: true

    property alias fieldObjectName: field.objectName
    property alias hintObjectName: hintLabel.objectName

    /// Whether the field has been given the cursor. What the hint
    /// follows, and what a page can ask before it decides the reader is
    /// done.
    ///
    /// `focus` rather than `activeFocus`: what the field was told, not
    /// whether the window agrees. Within a page they are the same thing
    /// -- focus is exclusive, so typing in the field below takes it away
    /// -- and `activeFocus` needs a window, which the tests have not
    /// got.
    readonly property bool editing: field.focus

    width: parent ? parent.width : 0
    height: column.height

    /// Put the cursor in the field, for a page that opens on an empty
    /// one.
    function edit() {
        field.forceActiveFocus()
    }

    /// Take it out again: what a page does on the way out, so the
    /// keyboard does not follow it.
    function done() {
        field.focus = false
    }

    Column {
        id: column
        width: parent.width

        // Full width, as Silica's fields are given it: a TextField keeps
        // its own text and its line inside `textLeftMargin`, so insetting
        // it here would inset it twice -- and the bio field below it on
        // the profile page would no longer line up with it.
        TextField {
            id: field
            width: parent.width
            horizontalAlignment: TextInput.AlignHCenter
            // The size and colour the name was drawn in when it was a
            // label: this is still a heading, whatever it is made of.
            font.pixelSize: Theme.fontSizeLarge
            color: Theme.highlightColor
            placeholderColor: Theme.secondaryHighlightColor
            placeholderText: root.fallbackText.length > 0 ? root.fallbackText
                                                          : root.placeholderText
            readOnly: !root.canEdit
            // Silica's own label would sit above the text, left-aligned,
            // under a centred name. The hint below says the same thing
            // where it reads.
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
