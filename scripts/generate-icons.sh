#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ICONS="$ROOT/assets/icons"
MASTER="$ICONS/super-platinum.svg"
PNG_DIR="$ICONS/png"
ICONSET="$ICONS/super-platinum.iconset"
ICON_DOC="$ICONS/super-platinum.icon"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: missing required tool: $1" >&2
    exit 1
  }
}

need rsvg-convert
need iconutil
need magick

mkdir -p "$PNG_DIR" "$ICONSET" "$ICONS/linux/hicolor"

echo "→ rasterizing master SVG"
for size in 16 32 48 64 128 256 512 1024; do
  rsvg-convert -w "$size" -h "$size" "$MASTER" -o "$PNG_DIR/icon_${size}.png"
done

cp "$PNG_DIR/icon_256.png" "$ICONS/icon-256.png"
cp "$PNG_DIR/icon_512.png" "$ICONS/icon-512.png"

echo "→ building super-platinum.icns (macOS classic)"
cp "$PNG_DIR/icon_16.png"   "$ICONSET/icon_16x16.png"
cp "$PNG_DIR/icon_32.png"   "$ICONSET/diana.ch@example.org"
cp "$PNG_DIR/icon_32.png"   "$ICONSET/icon_32x32.png"
cp "$PNG_DIR/icon_64.png"   "$ICONSET/ivan.p@example.net"
cp "$PNG_DIR/icon_128.png"  "$ICONSET/icon_128x128.png"
cp "$PNG_DIR/icon_256.png"  "$ICONSET/wendy.h@example.net"
cp "$PNG_DIR/icon_256.png"  "$ICONSET/icon_256x256.png"
cp "$PNG_DIR/icon_512.png"  "$ICONSET/wendy.h@example.net"
cp "$PNG_DIR/icon_512.png"  "$ICONSET/icon_512x512.png"
cp "$PNG_DIR/icon_1024.png" "$ICONSET/walt.e@example.net"
iconutil -c icns "$ICONSET" -o "$ICONS/super-platinum.icns"
rm -rf "$ICONSET"

echo "→ building super-platinum.ico (Windows)"
magick \
  "$PNG_DIR/icon_16.png" \
  "$PNG_DIR/icon_32.png" \
  "$PNG_DIR/icon_48.png" \
  "$PNG_DIR/icon_64.png" \
  "$PNG_DIR/icon_128.png" \
  "$PNG_DIR/icon_256.png" \
  "$ICONS/super-platinum.ico"

echo "→ linux hicolor theme icons"
for size in 16 32 48 64 128 256 512; do
  dir="$ICONS/linux/hicolor/${size}x${size}/apps"
  mkdir -p "$dir"
  cp "$PNG_DIR/icon_${size}.png" "$dir/super-platinum.png"
done
mkdir -p "$ICONS/linux/hicolor/scalable/apps"
cp "$MASTER" "$ICONS/linux/hicolor/scalable/apps/super-platinum.svg"

cat > "$ICONS/linux/super-platinum.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Super Platinum
Comment=A stupid fast and lightweight Slack client
Exec=super-platinum
Icon=super-platinum
Terminal=false
Categories=Network;InstantMessaging;Chat;
StartupWMClass=super-platinum
EOF

echo "→ compiling Liquid Glass Assets.car (macOS 26+)"
if [[ -d "$ICON_DOC" ]] && command -v xcrun >/dev/null 2>&1; then
  OUT="$ICONS/macos"
  mkdir -p "$OUT"
  PLIST="$OUT/assetcatalog_generated_info.plist"
  if xcrun actool "$ICON_DOC" \
    --compile "$OUT" \
    --output-format human-readable-text \
    --notices --warnings --errors \
    --output-partial-info-plist "$PLIST" \
    --app-icon super-platinum \
    --include-all-app-icons \
    --enable-on-demand-resources NO \
    --development-region en \
    --target-device mac \
    --minimum-deployment-target 26.0 \
    --platform macosx; then
    rm -f "$PLIST"
    if [[ -f "$OUT/Assets.car" ]]; then
      echo "   wrote $OUT/Assets.car"
    else
      echo "   warning: actool finished but Assets.car missing" >&2
    fi
  else
    echo "   warning: actool failed — classic .icns still available" >&2
  fi
else
  echo "   skip: xcrun/actool or super-platinum.icon not available"
fi

echo "done."
echo "  icns:  $ICONS/super-platinum.icns"
echo "  ico:   $ICONS/super-platinum.ico"
echo "  svg:   $ICONS/super-platinum.svg"
echo "  png:   $ICONS/icon-256.png"
echo "  glass: $ICONS/macos/Assets.car (if built)"
