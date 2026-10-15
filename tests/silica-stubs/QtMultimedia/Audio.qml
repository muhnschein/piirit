import QtQuick 2.0

// Stub: audio player. Real one needs sound backend, absent headless. Test
// drives these five members.
Item {
    // playbackState: QMediaPlayer values, 0 stopped, 1 playing, 2 paused.
    // Compared to 1 directly: enum not declarable on Qt 5.6.
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
