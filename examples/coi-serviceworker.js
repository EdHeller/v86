/* Makes the page cross-origin isolated (COOP/COEP) through a service worker, for static hosts that cannot send
   the headers. Same idea as gzuidhof/coi-serviceworker (MIT); a small re-implementation. The first load registers the
   worker and reloads once. */
if (typeof window === "undefined") {
    self.addEventListener("install", () => self.skipWaiting());
    self.addEventListener("activate", e => e.waitUntil(self.clients.claim()));
    self.addEventListener("fetch", e => {
        const r = e.request;
        if (r.cache === "only-if-cached" && r.mode !== "same-origin") return;
        e.respondWith(fetch(r).then(res => {
            if (res.status === 0) return res;
            const h = new Headers(res.headers);
            h.set("Cross-Origin-Embedder-Policy", "require-corp");
            h.set("Cross-Origin-Opener-Policy", "same-origin");
            h.set("Cross-Origin-Resource-Policy", "cross-origin");
            return new Response(res.body, { status: res.status, statusText: res.statusText, headers: h });
        }).catch(err => console.error(err)));
    });
}
else if (!window.crossOriginIsolated && window.isSecureContext && "serviceWorker" in navigator) {
    navigator.serviceWorker.register(document.currentScript.src).then(reg => {
        reg.addEventListener("updatefound", () => location.reload());
        if (reg.active && !navigator.serviceWorker.controller) location.reload();
    });
}
