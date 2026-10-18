import QtQuick 2.0
Item {
    default property alias contents: holder.data
    Item { id: holder }
    property bool busy: false
    // Silica: menu pulled out (read-only there; set by tests here).
    property bool active: false
}
