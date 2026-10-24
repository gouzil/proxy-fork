#!/usr/bin/env bash
set -euo pipefail
gui_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$gui_root"
gui_target="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
gui_profile="${2:-release}"
case "$gui_target" in aarch64-apple-darwin|x86_64-apple-darwin) ;; *) printf 'Unsupported macOS target: %s\n' "$gui_target" >&2; exit 1;; esac
case "$gui_profile" in release) gui_build=(--release);; debug) gui_build=();; *) printf 'Expected release or debug\n' >&2; exit 1;; esac
if [[ "${GUI_OFFICIAL_RELEASE:-0}" == 1 ]]; then
    : "${APPLE_SIGNING_IDENTITY:?Signing identity is required}"
    : "${APPLE_ID:?Notarization Apple ID is required}"
    : "${APPLE_TEAM_ID:?Apple team ID is required}"
    : "${APPLE_PASSWORD:?Notarization app password is required}"
fi
gui_version="$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -1)"
rg -q "version = \"$gui_version\"" crates/proxy-fork-gui/packager.toml || { printf 'Packager version differs from workspace\n' >&2; exit 1; }
export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --locked -p proxy-fork-gui --target "$gui_target" "${gui_build[@]}"
gui_output="$gui_root/target/gui-package/$gui_target/$gui_profile"
mkdir -p "$gui_output"
cp "$gui_root/target/$gui_target/$gui_profile/proxy-fork-gui" "$gui_output/proxy-fork-gui"
swift scripts/gui-icon.swift "$gui_root/target/gui-icon.png"
cd crates/proxy-fork-gui
cargo packager --config packager.toml --formats app --binaries-dir "$gui_root/target/$gui_target/$gui_profile" --out-dir "$gui_output" --target "$gui_target"
cd "$gui_root"
gui_bundle="$gui_output/Proxy Fork.app"
if [[ "${GUI_OFFICIAL_RELEASE:-0}" == 1 ]]; then
    codesign --force --deep --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$gui_bundle"
    ditto -c -k --keepParent "$gui_bundle" "$gui_output/notarize.zip"
    xcrun notarytool submit "$gui_output/notarize.zip" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_PASSWORD" --wait
    xcrun stapler staple "$gui_bundle"
    codesign --verify --deep --strict "$gui_bundle"
    spctl --assess --type execute "$gui_bundle"
else
    printf 'LOCAL TEST BUILD: unsigned or ad-hoc signed; not a formal release.\n'
fi
gui_asset="proxy-fork-gui-$gui_version-$gui_target.app.tar.gz"
COPYFILE_DISABLE=1 tar -czf "$gui_output/$gui_asset" -C "$gui_output" 'Proxy Fork.app'
(cd "$gui_output" && shasum -a 256 "$gui_asset" > "$gui_asset.sha256")
printf 'Packaged %s\n' "$gui_bundle"
