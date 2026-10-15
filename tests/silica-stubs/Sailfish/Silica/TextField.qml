import QtQuick 2.0

// Silica `EnterKey` attached property can't be stubbed: QML forbids capitalised
// names, qmetaobject can't register attached types. Pages using it won't load.
Item {
    property string text: ""
    property string label
    property string placeholderText
    property string description
    property int inputMethodHints
    property bool errorHighlight: false
    property bool readOnly: false
    property bool focus_: false
    property int horizontalAlignment: 0
    property bool labelVisible: true
    property int textTopMargin: 0
    property font font
    // Silica draws text and placeholder in own colours; name fields ask for
    // others.
    property color color: "#ffffff"
    property color placeholderColor: "#a0a0a0"
    signal clicked()
    implicitWidth: 400
    implicitHeight: 60
}
