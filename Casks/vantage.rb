cask "vantage" do
  version "0.5.0"
  sha256 "a33b5cae69c751bb9eaa32f064e5e98cd87e1093364d2489d584379be19c391a"

  url "https://github.com/beingbalveer/Vantage/releases/download/v#{version}/Vantage_#{version}_universal.dmg"
  name "Vantage"
  desc "Lightweight multi-repo Git client and code review deck"
  homepage "https://github.com/beingbalveer/Vantage"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true

  app "Vantage.app"

  postflight_steps do
    run "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "{{staged_path}}"]
  end

  zap trash: [
    "~/Library/Application Support/vantage",
    "~/Library/Caches/vantage",
    "~/Library/Caches/io.github.beingbalveer.vantage",
    "~/Library/Preferences/io.github.beingbalveer.vantage.plist",
    "~/Library/Saved Application State/io.github.beingbalveer.vantage.savedState",
  ]
end
