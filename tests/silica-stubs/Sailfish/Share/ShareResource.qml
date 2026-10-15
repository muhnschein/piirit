import QtQuick 2.0

// One shared thing: file by path, or text. Never app-made: platform hands them
// over. App tells kinds apart by which field is set. Test hands provider plain
// objects.
QtObject {
    property int type: 0
    property string filePath: ""
    property string data: ""
    property string status: ""
    property string linkTitle: ""
}
