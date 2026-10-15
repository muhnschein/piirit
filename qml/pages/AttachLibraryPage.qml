import QtQuick 2.0
import Sailfish.Pickers 1.0

/*
 * Attach from all indexed media (platform picker). Own page, by URL: missing
 * `Sailfish.Pickers` type costs one button, not the conversation.
 */
ContentPickerPage {
    id: picker

    /// Absolute path of chosen file.
    signal picked(string path)

    onSelectedContentPropertiesChanged: {
        // Core copies file; picked may vanish.
        if (selectedContentProperties.filePath) {
            picker.picked(selectedContentProperties.filePath)
        }
    }
}
