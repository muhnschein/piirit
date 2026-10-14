import QtQuick 2.0

// Silica multi-line field: grows with text; return inserts line break.
// `implicitHeight` grows per line; conversation caps it at page share.
Item {
    property string text: ""
    property string label
    property string placeholderText
    property string description
    property int inputMethodHints
    property bool errorHighlight: false
    property bool readOnly: false
    property bool labelVisible: true
    property int horizontalAlignment: 0
    property int textTopMargin: 0
    property font font
    property color color: "#ffffff"
    property color placeholderColor: "#a0a0a0"
    // Read-only: no focus unless set.
    property bool focusOnClick: !readOnly
    // Selection as Silica keeps; `select` and kin set it.
    property int selectionStart: 0
    property int selectionEnd: 0
    property string selectedText: ""
    property int cursorPosition: 0
    function select(start, end) {
        selectionStart = Math.min(start, end)
        selectionEnd = Math.max(start, end)
        // At `end`, where TextEdit leaves cursor.
        cursorPosition = end
        selectedText = text.substring(selectionStart, selectionEnd)
    }
    function selectAll() { select(0, text.length) }
    function deselect() { select(0, 0) }
    onTextChanged: deselect()
    signal clicked()
    implicitWidth: 400
    // One line plus one per line break.
    implicitHeight: 60 + 40 * (text.split("\n").length - 1)
}
