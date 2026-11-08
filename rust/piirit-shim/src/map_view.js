/*
 * Map integration only (`webxdc_host.rs`), run before app's own script. `?bounds=s,w,n,e`
 * (`maps.rs`) becomes view app restores at start (its `map.lat`, `map.lng`, `map.zoom`):
 * every location in sight, closest zoom that fits. Web Mercator, 256 px tiles, as Leaflet.
 */
(function (window) {
    "use strict";

    var area = String(new URLSearchParams(window.location.search).get("bounds") || "")
        .split(",")
        .map(parseFloat);
    if (area.length !== 4 || area.some(isNaN)) {
        return;
    }
    // Room round the edges for pins and names.
    var FILL = 0.8;
    // App's own closest zoom (focusZoomLevel).
    var CLOSEST = 18;
    // Mercator's edge.
    var EDGE = 85;

    function mercator(latitude) {
        var sine = Math.sin((Math.max(-EDGE, Math.min(EDGE, latitude)) * Math.PI) / 180);
        return Math.log((1 + sine) / (1 - sine)) / 2;
    }

    var south = mercator(area[0]);
    var north = mercator(area[2]);
    var wide = (area[3] - area[1]) / 360;
    var high = (north - south) / (2 * Math.PI);
    var fit = function (pixels, share) {
        return share > 0 ? Math.log2((pixels * FILL) / 256 / share) : CLOSEST;
    };
    var zoom = Math.floor(
        Math.min(fit(window.innerWidth || 320, wide), fit(window.innerHeight || 480, high))
    );
    zoom = Math.max(1, Math.min(CLOSEST, zoom));
    var middle = (south + north) / 2;
    try {
        window.localStorage.setItem("map.lat", (Math.atan(Math.sinh(middle)) * 180) / Math.PI);
        window.localStorage.setItem("map.lng", (area[1] + area[3]) / 2);
        window.localStorage.setItem("map.zoom", zoom);
    } catch (ignored) {
        // No storage: app's own start view.
    }
})(window);
