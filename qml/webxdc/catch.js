/*
 * Catches .xdc link tap before engine downloads it (to where app can't reach).
 * Frame script loaded by `WebxdcStorePage.qml`. Sailfish WebView has no
 * `shouldOverrideUrlLoading` (deltachat-android's hook), so click stopped here,
 * sent over message channel; page fetches via core (`WebxdcStore`). Other
 * links: engine as usual.
 */
(function () {
    "use strict";

    /*
     * Message name; engine's start "embed:", no clash.
     */
    var MESSAGE = "piirit:app";

    /*
     * Link under click, or "". Walks up from target (may hit a child).
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
     * App if path ends .xdc; query, fragment ignored (store links carry version).
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
        // Engine ignores click: file stays out of downloads.
        event.preventDefault();
        sendAsyncMessage(MESSAGE, { uri: address });
    }, true);
})();
