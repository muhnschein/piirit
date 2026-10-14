import QtQuick 2.0
import Sailfish.Pickers 1.0

/*
 * Webxdc app: platform file browser, .xdc files only. Library can't list
 * apps (not in media index).
 *
 * Own page, pushed by URL: `Sailfish.Pickers` types resolve at load; a
 * missing type costs one button, not the conversation.
 */
FilePickerPage {
    id: picker

    /// Absolute path of chosen app.
    signal picked(string path)

    // Heading: Silica's own, in every language.
    nameFilters: ["*.xdc"]

    onSelectedContentPropertiesChanged: {
        // Core copies file, so picked one may go away.
        if (selectedContentProperties.filePath) {
            picker.picked(selectedContentProperties.filePath)
        }
    }
}
