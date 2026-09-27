#!/usr/bin/env bash
# Build the wasm release and serve it at http://127.0.0.1:8080/
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if ! rustup target list --installed | grep -qx wasm32-unknown-unknown; then
    rustup target add wasm32-unknown-unknown
fi

cargo build --release --target wasm32-unknown-unknown

dest="$root/web/dist"
rm -rf "$dest"
mkdir -p "$dest"
cp "$root/web/index.html" "$root/web/mq_js_bundle.js" "$root/web/aether_host.js" "$dest/"
cp "$root/target/wasm32-unknown-unknown/release/aether.wasm" "$dest/"
touch "$dest/.nojekyll"

port="${1:-8080}"
echo "Aether: http://127.0.0.1:${port}/"
echo "Rovnou do simulace: http://127.0.0.1:${port}/?run=1"
exec python3 "$root/scripts/serve_static.py" "$dest" "$port"
