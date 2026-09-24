#!/usr/bin/env bash
# Rebuilds the vendored Amazon Chime SDK bundle that huddles load into the
# WebView. The output is committed so an ordinary `cargo build` needs neither
# node nor the network; rerun this only to move the pinned SDK version.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/vendor/chime"
OUT="$ROOT/assets/huddle/chime-sdk.min.js"

command -v npm >/dev/null 2>&1 || { echo "error: npm is required" >&2; exit 1; }

cd "$SRC"
npm ci --no-audit --no-fund
# `global` is referenced by the SDK's Node shims; without the define,
# constructing a DefaultMeetingSession throws `ReferenceError: global`.
npx esbuild entry.js --bundle --minify --format=iife --global-name=ChimeSDK \
  --platform=browser --target=safari15 --define:global=globalThis \
  --alias:@aws-sdk/client-chime-sdk-messaging=./messaging-stub.js \
  --legal-comments=eof --outfile="$OUT"
rm -rf "$SRC/node_modules"
echo "wrote $OUT ($(wc -c <"$OUT" | tr -d ' ') bytes)"
