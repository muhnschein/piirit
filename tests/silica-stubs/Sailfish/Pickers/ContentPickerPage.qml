import QtQuick 2.0
import Sailfish.Silica 1.0

// Silica picker over all indexed phone content. App reads one property: only
// that here. Test assigns it.
Page {
    property var selectedContentProperties: ({ filePath: "" })
}
