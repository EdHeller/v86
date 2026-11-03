#!/usr/bin/env python3
"""Serves this folder like python3 -m http.server, but the pages are cross-origin isolated (COOP/COEP headers),
which v86's cores in Web Workers need (SharedArrayBuffer).

    python3 serve.py                    # port 8000, isolated
    python3 serve.py 8000 --no-isolation   # without the headers: v86 then runs one core (the fallback)
"""
import http.server
import sys

args = [a for a in sys.argv[1:] if not a.startswith("--")]
PORT = int(args[0]) if args else 8000
ISOLATED = "--no-isolation" not in sys.argv


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm", ".js": "text/javascript"}

    def end_headers(self):
        if ISOLATED:
            self.send_header("Cross-Origin-Opener-Policy", "same-origin")
            self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
            self.send_header("Cross-Origin-Resource-Policy", "same-origin")
        super().end_headers()


if __name__ == "__main__":
    print(f"Serving on http://localhost:{PORT}/ ({'cross-origin isolated' if ISOLATED else 'not isolated'}; Ctrl-C to stop)")
    http.server.ThreadingHTTPServer(("", PORT), Handler).serve_forever()
