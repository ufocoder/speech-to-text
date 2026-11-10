#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
root=$(pwd)
if [ "$(uname -s)" != Darwin ]; then
    echo "macOS and Xcode are required to build the macOS apps." >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "Cargo was not found. Install Rust with rustup or add Cargo to PATH." >&2
    exit 1
fi

target_dir="$root/target"
output="$root/dist/macos"
version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n 1)
if [ -z "$version" ]; then
    echo "Could not determine the application version." >&2
    exit 1
fi

for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked --target "$target" --target-dir "$target_dir"
done

mkdir -p "$output"
stage=$(mktemp -d "$output/.build.XXXXXXXX")
trap 'rm -rf "$stage"' EXIT
trap 'exit 1' HUP INT TERM

package_app() {
    target=$1
    arch=$2
    app="$stage/$arch/Speech to Text.app"
    contents="$app/Contents"
    executable="$contents/MacOS/speech-to-text"

    mkdir -p "$contents/MacOS" "$contents/Resources"
    install -m 755 "$target_dir/$target/release/speech-to-text" "$executable"
    cp Info.plist "$contents/Info.plist"
    cp -R macos/Resources/. "$contents/Resources/"
    plutil -insert CFBundleIdentifier -string 'local.speech-to-text' "$contents/Info.plist"
    plutil -insert CFBundleDisplayName -string 'Speech to Text' "$contents/Info.plist"
    plutil -insert CFBundleIconFile -string 'Microphone.icns' "$contents/Info.plist"
    plutil -insert CFBundleExecutable -string 'speech-to-text' "$contents/Info.plist"
    plutil -insert CFBundlePackageType -string 'APPL' "$contents/Info.plist"
    plutil -insert CFBundleShortVersionString -string "$version" "$contents/Info.plist"
    plutil -insert CFBundleVersion -string "$version" "$contents/Info.plist"
    plutil -insert CFBundleDevelopmentRegion -string 'en' "$contents/Info.plist"
    plutil -lint "$contents/Info.plist"
    lipo "$executable" -verify_arch "$arch"
    codesign --force --sign - "$app"
    codesign --verify --strict "$app"
}

package_app aarch64-apple-darwin arm64
package_app x86_64-apple-darwin x86_64

for arch in arm64 x86_64; do
    mkdir -p "$output/$arch"
    app="$output/$arch/Speech to Text.app"
    rm -rf "$app"
    mv "$stage/$arch/Speech to Text.app" "$app"
    printf 'Built %s\n' "$app"
done
