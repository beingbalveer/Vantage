cask "vantage" do
  version "0.4.0"
  sha256 "682399576b5c2485eea77ead8414d899ad12782082d533b4bd8065952d145133"

  url "https://github.com/beingbalveer/Vantage/releases/download/v#{version}/Vantage_#{version}_universal.dmg"
  name "Vantage"
  desc "Lightweight multi-repo Git client and code review deck"
  homepage "https://github.com/beingbalveer/Vantage"

  app "Vantage.app"

  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/Vantage.app"],
                   sudo: false
  end

  zap trash: [
    "~/Library/Application Support/vantage",
    "~/Library/Caches/vantage",
    "~/Library/Caches/io.github.beingbalveer.vantage",
    "~/Library/Preferences/io.github.beingbalveer.vantage.plist",
    "~/Library/Saved Application State/io.github.beingbalveer.vantage.savedState",
  ]
end
