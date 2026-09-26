#!/usr/bin/env python3
"""Tiny static server that serves .wasm as application/wasm."""

import http.server
import mimetypes
import os
import socketserver
import sys

mimetypes.add_type("application/wasm", ".wasm")
mimetypes.add_type("application/javascript", ".js")
mimetypes.add_type("text/html", ".html")


def main() -> None:
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} DIR PORT", file=sys.stderr)
        sys.exit(2)
    root = os.path.abspath(sys.argv[1])
    port = int(sys.argv[2])
    os.chdir(root)
    handler = http.server.SimpleHTTPRequestHandler
    with socketserver.TCPServer(("127.0.0.1", port), handler) as httpd:
        httpd.serve_forever()


if __name__ == "__main__":
    main()
