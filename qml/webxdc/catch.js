/*
 * Tap on app link before engine downloads it.
 *
 * Frame script in engine world (loaded by `WebxdcStorePage.qml`), beside store page.
 * Engine would save .xdc download where app can't reach. deltachat-android decides in
 * `shouldOverrideUrlLoading`; Sailfish WebView has no hook. So: stop click on app link
 * here, send over message channel; page fetches via core (`WebxdcStore`).
 *
 * Only .xdc links. Store pages, search, other links: engine as usual.
 */
(function () {
    "use strict";

    /*
     * App's message name: engine messages all start with "embed:", so no clash.
     */
    var MESSAGE = "piirit:app";

    /*
     * Link address under click, or "" if none. Walks up from target (click may hit a child).
     */
    function linkFrom(node) {
        while (node) {
            var name = node.localName;
            if ((name === "a" || name === "area") && node.href) {
                return "" + node.href;
            }
            node = node.parentNode;
        }
        return "";
    }

    /*
     * Address is app if path ends .xdc. Query and fragment ignored (store links carry version).
     */
    function isApp(address) {
        var path = address.split("#")[0].split("?")[0];
        return path.slice(-4).toLowerCase() === ".xdc";
    }

    /*
     * Capture phase: page handlers can't swallow the click first.
     */
    addEventListener("click", function (event) {
        var address = linkFrom(event.target);
        if (!address || !isApp(address)) {
            return;
        }
        // Engine does nothing with click: keeps file out of downloads. App fetches via core.
        event.preventDefault();
        sendAsyncMessage(MESSAGE, { uri: address });
    }, true);
})();
