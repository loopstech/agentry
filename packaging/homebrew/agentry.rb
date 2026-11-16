# Template: the Homebrew workflow fills in {{VERSION}}, {{SHA256}} and {{REPO}}.
cask "agentry" do
  version "{{VERSION}}"
  sha256 "{{SHA256}}"

  url "https://github.com/{{REPO}}/releases/download/v#{version}/Agentry.dmg"
  name "Agentry"
  desc "See and steer every Claude Code session from one widget"
  homepage "https://github.com/{{REPO}}"

  depends_on macos: ">= :monterey"

  app "Agentry.app"
  binary "#{appdir}/Agentry.app/Contents/MacOS/agentry"

  # Remove Agentry's Claude Code hooks (and nothing else) before uninstalling.
  uninstall_preflight do
    system_command "#{appdir}/Agentry.app/Contents/MacOS/agentry",
                   args: ["hooks", "uninstall"],
                   must_succeed: false
  end

  zap trash: [
    "~/.agentry",
    "~/Library/Application Support/io.loopstech.agentry",
    "~/Library/Caches/io.loopstech.agentry",
    "~/Library/LaunchAgents/Agentry.plist",
  ]
end
