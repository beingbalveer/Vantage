# frozen_string_literal: true

cask "vantage" do
  version "0.6.0"
  sha256 "907bd9a46c71fc48adca70223bbe596b542614ae0736300d83254634a1cdef22"

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
    "~/Library/Caches/io.github.beingbalveer.vantage",
    "~/Library/Caches/vantage",
    "~/Library/Preferences/io.github.beingbalveer.vantage.plist",
    "~/Library/Saved Application State/io.github.beingbalveer.vantage.savedState",
  ]
end
