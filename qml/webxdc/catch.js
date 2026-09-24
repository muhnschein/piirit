/*
 * Take a tap on an app before the browser engine can download it.
 *
 * This is not part of the interface: it is a *frame script*, loaded into
 * the browser engine's own world by `WebxdcStorePage.qml` and running
 * beside the store's page rather than inside this app. Nothing else in
 * qml/ is JavaScript for the engine, which is why it is here on its own.
 *
 * The store is an ordinary website, and a link to an app is a link to a
 * .xdc file. Following one is a download to the engine: it saves the
 * file somewhere this app cannot reach and there is nothing to show for
 * the tap. deltachat-android answers this in `shouldOverrideUrlLoading`
 * -- every navigation is the app's to decide, and an .xdc is fetched
 * through the core instead. Sailfish's WebView has no such hook, so the
 * decision is made where the tap happens: a click on a link to an app is
 * stopped here and handed back over the message channel, and the page
 * fetches it through the core (`WebxdcStore`).
 *
 * Only .xdc links are taken. Everything else -- the store's own pages,
 * its search, a link to somebody's homepage -- is left to the engine, so
 * the store browses as a website should.
 */
(function () {
    "use strict";

    /*
     * What the page tells the app when a link to an app was tapped. The
     * name is this app's, so nothing the engine sends can be mistaken for
     * it: every message the engine defines begins with "embed:".
     */
    var MESSAGE = "piirit:app";

    /*
     * The address of the link a click landed in, or "" for a click that
     * was not on one. The click can land on anything inside the link --
     * a picture, a word, a box drawn around both -- so this walks up
     * until it finds the link or runs out of page.
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
     * Whether an address points at an app. The query and the fragment are
     * not part of the name: the store's own links carry a version in the
     * query.
     */
    function isApp(address) {
        var path = address.split("#")[0].split("?")[0];
        return path.slice(-4).toLowerCase() === ".xdc";
    }

    /*
     * Capture rather than bubble: the page's own handlers should not get
     * to swallow the click before this sees it.
     */
    addEventListener("click", function (event) {
        var address = linkFrom(event.target);
        if (!address || !isApp(address)) {
            return;
        }
        // The engine is told to do nothing about this click, which is
        // what keeps the file out of the browser's downloads; the app
        // fetches it through the core instead.
        event.preventDefault();
        sendAsyncMessage(MESSAGE, { uri: address });
    }, true);
})();
