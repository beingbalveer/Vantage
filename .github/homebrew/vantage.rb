cask "vantage" do
  arch arm: "aarch64", intel: "x64"

  version "0.2.0"
  sha256 :no_check # Or specify the exact sha256 checksum of the release DMG

  url "https://github.com/beingbalveer/Vantage/releases/download/v#{version}/Vantage_#{version}_universal.dmg"
  name "Vantage"
  desc "Fast, local, multi-repo Git desktop client, read-only code viewer, and terminal workspace"
  homepage "https://github.com/beingbalveer/Vantage"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true

  app "Vantage.app"

  # Bundle id comes from src-tauri/tauri.conf.json → "identifier"
  # (io.github.beingbalveer.vantage). Keep these in sync.
  zap trash: [
    "~/Library/Application Support/io.github.beingbalveer.vantage",
    "~/Library/Application Support/vantage",
    "~/Library/Caches/io.github.beingbalveer.vantage",
    "~/Library/Preferences/io.github.beingbalveer.vantage.plist",
    "~/Library/Saved Application State/io.github.beingbalveer.vantage.savedState",
  ]
end
