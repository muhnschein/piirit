import QtQuick 2.0

// Sailfish's Gecko view (browser-engine release only). App uses: where to go,
// whether arrived, tapped link, frame script messages. Loads nothing (real
// fetch/draw is a device question). `active` absent from what app sets:
// Silica's WebView.qml binds it; overriding it left a view that never rendered
// (docs/PROJECT.md).
Item {
    property url url
    property bool active: false
    property bool loading: false
    property bool loaded: false
    property int loadProgress: 0
    property string title: ""
    property bool canGoBack: false

    // Registered and loaded state, kept private by real view. A page forgetting
    // either gets no frame script messages and no clue why.
    property string listeners: ""
    property string frameScripts: ""

    // Raised on link tap, before engine picks type. Only moment app hears of
    // downloads.
    signal linkClicked(string url)

    // Frame script message.
    signal recvAsyncMessage(string message, var data)

    function addMessageListener(name) { listeners += name + ";" }
    function loadFrameScript(where) { frameScripts += where + ";" }
    function sendAsyncMessage(name, data) {}

    function reload() {}
    function stop() {}
    function goBack() {}
}
