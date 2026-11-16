#!/usr/bin/env bash
#
# Package the release binaries as a universal macOS app bundle, a notarizable
# DMG, a zipped bundle and a matching Homebrew cask.
#
#   scripts/bundle-macos.sh [version]
#
# `version` defaults to the workspace version in Cargo.toml, or to
# `$GITHUB_REF_NAME` without its leading `v` in Actions.
#
# Inputs: `target/{aarch64,x86_64}-apple-darwin/release/vantage`. Build them with
#
#   cargo build --release -p vantage --locked --target aarch64-apple-darwin
#   cargo build --release -p vantage --locked --target x86_64-apple-darwin
#
# The script falls back to `target/release/vantage` when only one architecture
# exists locally, so a developer can smoke-test the packaging on its own.
#
# Signing (optional, from the environment):
#   APPLE_SIGNING_IDENTITY   "Developer ID Application: Name (TEAMID)".
#                            Without it the bundle is ad-hoc signed, which is
#                            enough to run locally but not to distribute.
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD
#                            notarytool credentials. When all three are set the
#                            app and the DMG are notarized and stapled.
#
# Published builds rely on signing and notarization: the generated cask leaves
# macOS's quarantine attribute in place instead of bypassing Gatekeeper. An
# ad-hoc build installed from that cask needs right-click > Open on first launch.
#
# Output (in dist/): Vantage.app, Vantage_<version>_universal.dmg,
# Vantage_<version>_universal.app.zip, vantage.rb, SHA256SUMS.

set -euo pipefail

cd "$(dirname "$0")/.."

BUNDLE_ID="io.github.beingbalveer.vantage"
APP_NAME="Vantage"
ICON_SRC="assets/vantage-icon-1024.png"
DIST="dist"
ARM64_BIN="target/aarch64-apple-darwin/release/vantage"
X86_64_BIN="target/x86_64-apple-darwin/release/vantage"
HOST_BIN="target/release/vantage"

die() {
    echo "bundle-macos: $*" >&2
    exit 1
}

step() {
    echo "bundle-macos: $*"
}

# ---- version ---------------------------------------------------------------

version="${1:-}"
if [ -z "$version" ]; then
    version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
fi
[ -n "$version" ] || die "could not determine the version"
# The release workflow passes the tag; accept both `0.6.0` and `v0.6.0`.
version="${version#v}"

# ---- universal binary ------------------------------------------------------

if [ -f "$ARM64_BIN" ] && [ -f "$X86_64_BIN" ]; then
    work_dir="$(mktemp -d)"
    universal="$work_dir/vantage"
    lipo -create "$ARM64_BIN" "$X86_64_BIN" -output "$universal"
    archs="universal"
elif [ -f "$ARM64_BIN" ]; then
    universal="$ARM64_BIN"
    archs="arm64"
elif [ -f "$X86_64_BIN" ]; then
    universal="$X86_64_BIN"
    archs="x86_64"
elif [ -f "$HOST_BIN" ]; then
    universal="$HOST_BIN"
    archs="host"
else
    die "no release binary found; build with --target aarch64-apple-darwin and x86_64-apple-darwin"
fi
[ -f "$ICON_SRC" ] || die "$ICON_SRC is missing"
[ "$archs" = "universal" ] || step "warning: packaging $archs only (one architecture was built)"
step "packaging $APP_NAME $version ($(lipo -archs "$universal"))"

rm -rf "$DIST/$APP_NAME.app" "$DIST/dmg"
mkdir -p "$DIST"

# ---- bundle ----------------------------------------------------------------

mkdir -p "$DIST/$APP_NAME.app/Contents/MacOS" "$DIST/$APP_NAME.app/Contents/Resources"
cp "$universal" "$DIST/$APP_NAME.app/Contents/MacOS/vantage"
chmod +x "$DIST/$APP_NAME.app/Contents/MacOS/vantage"
printf 'APPL????' >"$DIST/$APP_NAME.app/Contents/PkgInfo"

iconset="$(mktemp -d)/Vantage.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$ICON_SRC" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$ICON_SRC" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$DIST/$APP_NAME.app/Contents/Resources/$APP_NAME.icns"

cat >"$DIST/$APP_NAME.app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>vantage</string>
    <key>CFBundleIconFile</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
PLIST

# ---- sign ------------------------------------------------------------------

sign_app() {
    if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
        step "signing with $APPLE_SIGNING_IDENTITY"
        codesign --force --deep --options runtime --timestamp \
            --sign "$APPLE_SIGNING_IDENTITY" "$DIST/$APP_NAME.app"
    else
        step "APPLE_SIGNING_IDENTITY unset: ad-hoc signing (not distributable)"
        codesign --force --deep --sign - --timestamp=none "$DIST/$APP_NAME.app"
    fi
}
sign_app

notarize_ready() {
    [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]
}

notarize() {
    step "notarizing $1"
    xcrun notarytool submit "$1" \
        --apple-id "$APPLE_ID" \
        --team-id "$APPLE_TEAM_ID" \
        --password "$APPLE_APP_PASSWORD" \
        --wait
    xcrun stapler staple "$1"
    xcrun stapler validate "$1"
}

# The app is submitted as a zip so the ticket can be stapled to the bundle; the
# zip is rebuilt afterwards so it carries the stapled bundle.
zip_app() {
    ditto -c -k --sequesterRsrc --keepParent \
        "$DIST/$APP_NAME.app" "$DIST/Vantage_${version}_universal.app.zip"
}

if notarize_ready && [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
    zip_app
    notarize "$DIST/Vantage_${version}_universal.app.zip"
else
    step "notarytool credentials unset: skipping notarization (local build)"
fi
zip_app

# ---- dmg -------------------------------------------------------------------

dmg="Vantage_${version}_universal.dmg"
mkdir -p "$DIST/dmg"
cp -R "$DIST/$APP_NAME.app" "$DIST/dmg/"
ln -s /Applications "$DIST/dmg/Applications"
hdiutil create \
    -volname "$APP_NAME" \
    -srcfolder "$DIST/dmg" \
    -ov -format UDZO \
    "$DIST/$dmg" >/dev/null
rm -rf "$DIST/dmg"

if notarize_ready && [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
    notarize "$DIST/$dmg"
fi

# ---- checksums + cask ------------------------------------------------------

(cd "$DIST" && shasum -a 256 "$dmg" "Vantage_${version}_universal.app.zip" >SHA256SUMS)
dmg_sha="$(shasum -a 256 "$DIST/$dmg" | awk '{print $1}')"

cat >"$DIST/vantage.rb" <<CASK
# frozen_string_literal: true

cask "vantage" do
  version "$version"
  sha256 "$dmg_sha"

  url "https://github.com/beingbalveer/Vantage/releases/download/v#{version}/Vantage_#{version}_universal.dmg"
  name "Vantage"
  desc "Multi-repo Git dashboard: branches, changes, commits and sync in one window"
  homepage "https://github.com/beingbalveer/Vantage"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :ventura"

  app "Vantage.app"

  # A published build relies on Developer ID signing and notarization for
  # Gatekeeper approval; the quarantine attribute is intentionally left in place.
  zap trash: [
    "~/Library/Application Support/vantage",
    "~/Library/Caches/$BUNDLE_ID",
    "~/Library/Caches/vantage",
    "~/Library/Preferences/$BUNDLE_ID.plist",
    "~/Library/Saved Application State/$BUNDLE_ID.savedState",
  ]
end
CASK

echo
step "wrote"
echo "  $DIST/$APP_NAME.app"
echo "  $DIST/$dmg   (sha256 $dmg_sha)"
echo "  $DIST/Vantage_${version}_universal.app.zip"
echo "  $DIST/SHA256SUMS"
echo "  $DIST/vantage.rb"
