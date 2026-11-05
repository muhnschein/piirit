// Auto-transcribe choices, words shared by setup dialog, settings page, transcription page.
// No `.pragma library`: qsTr() needs importing file's context.

/// Choice's name: 0 never, 1 while charging, 2 always.
function modeName(mode) {
    if (mode === 1) {
        //: Auto-transcribe voice messages: only while the phone is on its charger.
        return qsTr("While charging")
    }
    if (mode === 2) {
        //: Auto-transcribe voice messages: as soon as they have downloaded.
        return qsTr("Always")
    }
    //: Auto-transcribe voice messages: never, only by a tap on the message.
    return qsTr("Never")
}

/// One sentence under choice.
function modeDescription(mode) {
    if (mode === 1) {
        return qsTr("New voice messages are transcribed while the phone is charging.")
    }
    if (mode === 2) {
        // One sentence per string (docs/PROJECT.md).
        return qsTr("New voice messages are transcribed once they have downloaded.") + " "
               + qsTr("This uses battery.")
    }
    return qsTr("Tap Transcribe on a voice message to turn it into text.")
}
