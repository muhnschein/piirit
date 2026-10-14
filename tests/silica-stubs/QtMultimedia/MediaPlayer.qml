import QtQuick 2.0

// Video player stub: no media backend headless, codecs vary by machine.
// `playbackState`: QMediaPlayer values, 0 stopped, 1 playing, 2 paused (enum
// not declarable on Qt 5.6).
Item {
    property url source
    property bool autoPlay: false
    property int playbackState: 0
    property int position: 0
    property int duration: 0

    function play() { playbackState = 1 }
    function pause() { playbackState = 2 }
    function stop() { playbackState = 0; position = 0 }
    function seek(offset) { position = offset }
}
