import QtQuick 2.0
import Sailfish.Pickers 1.0

/*
 * Webxdc app: file browser, .xdc only (not in media index). Own page, by URL:
 * `Sailfish.Pickers` resolve at load; missing costs a button, not conversation.
 */
FilePickerPage {
    id: picker

    /// Absolute path of chosen app.
    signal picked(string path)

    // Heading: Silica's own, in every language.
    nameFilters: ["*.xdc"]

    onSelectedContentPropertiesChanged: {
        // Core copies file; picked one may go.
        if (selectedContentProperties.filePath) {
            picker.picked(selectedContentProperties.filePath)
        }
    }
}
