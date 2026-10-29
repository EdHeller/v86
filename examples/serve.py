#!/usr/bin/env python3
# Static server for the demo. Sends COOP/COEP (the page is cross-origin isolated), listens on IPv4 and IPv6 and prints the addresses it serves.
# usage: python3 serve.py [port]
import http.server, os, socket, sys

class H(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cross-Origin-Resource-Policy", "cross-origin")
        super().end_headers()

class Server(http.server.ThreadingHTTPServer):
    address_family = socket.AF_INET6
    def server_bind(self):
        try:
            self.socket.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 0)   # IPv4 too
        except OSError:
            pass
        super().server_bind()

port = int(sys.argv[1]) if len(sys.argv) > 1 else 8000
os.chdir(os.path.dirname(os.path.abspath(__file__)))
try:
    server = Server(("::", port), H)
    dual = True
except OSError:
    server = http.server.ThreadingHTTPServer(("0.0.0.0", port), H)   # no IPv6 here
    dual = False
print("Serving this folder on port %d" % port)
print("  http://localhost:%d/            64-bit (x86_64)" % port)
print("  http://127.0.0.1:%d/" % port)
if dual:
    print("  http://[::1]:%d/" % port)
print("Parameters (separate them with &):")
print("  ?arch=x86                32-bit        ?cores=4          four cores that take turns (?cores works with ?arch=x86 too: ?arch=x86&cores=4)")
print("  ?wasm=prev               previous build for comparison      ?acpi=1  ACPI + APIC")
print("Stop with Ctrl-C.")
try:
    server.serve_forever()
except KeyboardInterrupt:
    print()
