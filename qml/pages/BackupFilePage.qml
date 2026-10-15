import QtQuick 2.0
import Sailfish.Pickers 1.0

/*
 * Backup picker: platform browser, only files a Delta Chat backup can be. Own page:
 * `Sailfish.Pickers` types resolve at load; missing type costs one button, not the page.
 */
FilePickerPage {
    id: picker

    /// No quick actions: first step of restoring a backup. See piirit.qml.
    readonly property bool pausesQuickActions: true

    /// Absolute path of chosen backup.
    signal picked(string path)

    // Core writes .tar via `export_backup`: the format since change.
    nameFilters: ["*.tar"]

    onSelectedContentPropertiesChanged: {
        if (selectedContentProperties.filePath) {
            picker.picked(selectedContentProperties.filePath)
        }
    }
}
