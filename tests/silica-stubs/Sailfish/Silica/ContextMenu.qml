import QtQuick 2.0
Item {
    default property alias contents: holder.data
    Item { id: holder }
    // Silica closes menu on MenuItem tap; other calls must ask. Nothing to
    // close here, but call must resolve or handler stops at that line.
    function close() {}
}
