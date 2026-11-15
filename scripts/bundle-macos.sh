#!/usr/bin/env bash
#
# Package the release binary as a macOS app bundle, a DMG and a matching cask.
#
#   scripts/bundle-macos.sh [version]
#
# `version` defaults to the workspace version in Cargo.toml, or to
# `$GITHUB_REF_NAME` without its leading `v` when running in Actions.
#
# Output (in dist/):
#   Vantage.app                        the bundle itself
#   Vantage_<version>_arm64.dmg        drag-to-Applications installer
#   Vantage_<version>_arm64.app.zip    zipped bundle (Homebrew / manual install)
#   vantage.rb                         cask with the real sha256 filled in
#
# Everything here uses tools that ship with macOS (sips, iconutil, hdiutil,
# codesign, ditto), so the workflow needs no extra installer steps.

set -euo pipefail

cd "$(dirname "$0")/.."

BUNDLE_ID="io.github.beingbalveer.vantage"
APP_NAME="Vantage"
BIN="target/release/vantage"
ICON_SRC="assets/vantage-icon-1024.png"
DIST="dist"

die() {
    echo "bundle-macos: $*" >&2
    exit 1
}

# ---- version ---------------------------------------------------------------

version="${1:-}"
if [ -z "$version" ]; then
    version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
fi
if [ -z "$version" ]; then
    die "could not determine the version"
fi
# The release workflow passes the tag; accept both `0.6.0` and `v0.6.0`.
version="${version#v}"

# ---- preconditions ---------------------------------------------------------

[ -f "$BIN" ] || die "$BIN is missing; run 'cargo build --release -p vantage' first"
[ -f "$ICON_SRC" ] || die "$ICON_SRC is missing"
arch="$(lipo -archs "$BIN")"
echo "bundle-macos: packaging $APP_NAME $version ($arch)"

rm -rf "$DIST/$APP_NAME.app" "$DIST/dmg"
mkdir -p "$DIST"

# ---- icon ------------------------------------------------------------------

iconset="$(mktemp -d)/Vantage.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$ICON_SRC" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$ICON_SRC" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
mkdir -p "$DIST/$APP_NAME.app/Contents/Resources"
iconutil -c icns "$iconset" -o "$DIST/$APP_NAME.app/Contents/Resources/$APP_NAME.icns"

# ---- bundle ----------------------------------------------------------------

mkdir -p "$DIST/$APP_NAME.app/Contents/MacOS"
cp "$BIN" "$DIST/$APP_NAME.app/Contents/MacOS/vantage"
chmod +x "$DIST/$APP_NAME.app/Contents/MacOS/vantage"
printf 'APPL????' >"$DIST/$APP_NAME.app/Contents/PkgInfo"

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

# Ad-hoc signature: an unsigned bundle is refused outright by recent macOS,
# and a self-signed one also lets `codesign --verify` gate the release.
codesign --force --sign - --timestamp=none "$DIST/$APP_NAME.app" >/dev/null 2>&1 ||
    echo "bundle-macos: warning: ad-hoc codesign failed (bundle left unsigned)"

# ---- dmg + zip -------------------------------------------------------------

dmg="Vantage_${version}_arm64.dmg"
zip="Vantage_${version}_arm64.app.zip"

mkdir -p "$DIST/dmg"
cp -R "$DIST/$APP_NAME.app" "$DIST/dmg/"
ln -s /Applications "$DIST/dmg/Applications"
hdiutil create \
    -volname "$APP_NAME" \
    -srcfolder "$DIST/dmg" \
    -ov -format UDZO \
    "$DIST/$dmg" >/dev/null
rm -rf "$DIST/dmg"

ditto -c -k --sequesterRsrc --keepParent "$DIST/$APP_NAME.app" "$DIST/$zip"

dmg_sha="$(shasum -a 256 "$DIST/$dmg" | awk '{print $1}')"

# ---- cask ------------------------------------------------------------------

cat >"$DIST/vantage.rb" <<CASK
cask "vantage" do
  version "$version"
  sha256 "$dmg_sha"

  url "https://github.com/beingbalveer/Vantage/releases/download/v#{version}/Vantage_#{version}_arm64.dmg"
  name "Vantage"
  desc "Multi-repo Git dashboard: branches, changes, commits and sync in one window"
  homepage "https://github.com/beingbalveer/Vantage"

  depends_on arch: :arm64
  depends_on macos: ">= :ventura"

  app "Vantage.app"

  postflight_steps do
    run "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "{{staged_path}}"]
  end

  zap trash: [
    "~/Library/Application Support/vantage",
    "~/Library/Caches/vantage",
    "~/Library/Caches/$BUNDLE_ID",
    "~/Library/Preferences/$BUNDLE_ID.plist",
    "~/Library/Saved Application State/$BUNDLE_ID.savedState",
  ]
end
CASK

echo
echo "bundle-macos: wrote"
echo "  $DIST/$APP_NAME.app"
echo "  $DIST/$dmg   (sha256 $dmg_sha)"
echo "  $DIST/$zip"
echo "  $DIST/vantage.rb"
