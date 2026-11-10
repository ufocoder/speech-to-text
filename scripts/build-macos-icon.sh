#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
if ! command -v rsvg-convert >/dev/null 2>&1; then
    echo "Install librsvg (brew install librsvg) to regenerate the app icon." >&2
    exit 1
fi

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
trap 'exit 1' HUP INT TERM
iconset="$stage/Microphone.iconset"
mkdir "$iconset"

for size in 16 32 128 256 512; do
    rsvg-convert -w "$size" -h "$size" -o "$iconset/icon_${size}x${size}.png" macos/Assets/Microphone.svg
    retina=$((size * 2))
    rsvg-convert -w "$retina" -h "$retina" -o "$iconset/icon_${size}x${size}@2x.png" macos/Assets/Microphone.svg
done

iconutil -c icns -o "$stage/Microphone.icns" "$iconset"
install -m 644 "$stage/Microphone.icns" macos/Resources/Microphone.icns
