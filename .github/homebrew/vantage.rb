cask "vantage" do
  arch arm: "aarch64", intel: "x64"

  version "0.1.0"
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

  zap trash: [
    "~/Library/Application Support/com.beingbalveer.vantage",
    "~/Library/Application Support/vantage",
    "~/Library/Caches/com.beingbalveer.vantage",
    "~/Library/Preferences/com.beingbalveer.vantage.plist",
    "~/Library/Saved Application State/com.beingbalveer.vantage.savedState",
  ]
end
