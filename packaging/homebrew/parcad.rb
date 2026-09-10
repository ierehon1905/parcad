# The Homebrew cask, kept here as the source of truth and copied into the tap
# repository (ierehon1905/homebrew-parcad) as Casks/parcad.rb on each release.
#
# The point of the cask is not convenience: `brew install --cask` clears the
# quarantine flag, so an unsigned build installs without macOS calling it
# damaged. Until there is a Developer ID, this is the only install path that
# does not need a terminal incantation first.
#
# sha256 comes from SHA256SUMS.txt on the release.
cask "parcad" do
  version "0.0.1"
  sha256 "8522ab8425a4618a382d5a45c3ba4d25650770b5b115361935e8ec5916c0afd0"

  url "https://github.com/ierehon1905/parcad/releases/download/v#{version}/ParCAD-aarch64-apple-darwin.zip",
      verified: "github.com/ierehon1905/parcad/"
  name "ParCAD"
  desc "Parametric CAD you write as code"
  homepage "https://github.com/ierehon1905/parcad"

  depends_on arch: :arm64
  depends_on macos: ">= :big_sur"

  app "ParCAD.app"

  # Deliberately not listing ~/Documents/parcad. That folder is the user's own
  # parts, not application state, and an uninstall must never take it.
  zap trash: [
    "~/Library/Application Support/dev.parcad.app",
    "~/Library/Caches/dev.parcad.app",
    "~/Library/Saved Application State/dev.parcad.app.savedState",
    "~/Library/WebKit/dev.parcad.app",
  ]
end
