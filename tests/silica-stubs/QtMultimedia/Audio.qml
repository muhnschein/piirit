import QtQuick 2.0

// QtMultimedia's audio player. Stubbed rather than pulled in as a real
// dependency: a headless runner has no audio backend, so the real one would
// make every conversation test depend on the machine's sound stack. What
// the preview reads is these five members, and a test drives them by
// assigning to them.
Item {
    // playbackState carries QMediaPlayer's own values: 0 stopped, 1
    // playing, 2 paused. AttachmentPreview compares against 1 directly,
    // because the real enum lives on the type and a QML component cannot
    // declare one on Qt 5.6.
    property url source
    property bool autoPlay: false
    property bool autoLoad: true
    property int playbackState: 0
    property int position: 0
    property int duration: 0

    function play() { playbackState = 1 }
    function pause() { playbackState = 2 }
    function stop() { playbackState = 0; position = 0 }
}
