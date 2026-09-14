# The Homebrew formula, kept here as the source of truth and copied into the
# tap repository (ierehon1905/homebrew-parcad) as Formula/parcad.rb on each
# release. It replaced a cask on 2026-09-14: a cask installs a window, and what
# a Homebrew user of this wants is the host — MCP up at login, the UI in a
# browser — which `parcad serve` is. A formula is also never quarantined, so
# the `xattr` step the .app needs does not exist on this path.
#
# The tarball is the one .github/workflows/release.yml stages: `parcad`, its
# B-rep worker, the seed parts and the licence texts. sha256 comes from
# SHA256SUMS.txt on the release. Nothing here is built from source — a cold
# OpenCASCADE compile is not something to ask of an install.
class Parcad < Formula
  desc "Parametric CAD you write as code, with an MCP server for the model that writes it"
  homepage "https://github.com/ierehon1905/parcad"
  version "0.0.3"
  url "https://github.com/ierehon1905/parcad/releases/download/v#{version}/parcad-cli-aarch64-apple-darwin.tar.gz"
  sha256 "cad4f8ce4c8dd5a7631078917bcb17bd4289c4af83772fdb2aa1a467f8645582"
  # The Rust crates are MIT or Apache-2.0; the statically linked OpenCASCADE
  # inside the worker is LGPL-2.1 with its exception. The texts are installed
  # beside the binaries, and NOTICE.md says how to relink your own OCCT.
  license any_of: ["MIT", "Apache-2.0"]

  depends_on arch: :arm64
  depends_on :macos

  def install
    # Both binaries in libexec: the host finds its worker beside itself, and
    # the env script below names it explicitly as well.
    libexec.install "parcad", "parcad-occt-worker"
    (share/"parcad").install "examples"
    doc.install Dir["licenses/*"]
    (bin/"parcad").write_env_script libexec/"parcad",
      PARCAD_OCCT_WORKER: libexec/"parcad-occt-worker",
      PARCAD_SEED_DIR:    share/"parcad/examples"
  end

  # `brew services start parcad`: the UI on http://127.0.0.1:4242 and MCP on
  # /mcp, up at login. Parts live in ~/Documents/parcad, as they do for the
  # app; PARCAD_PROJECTS_DIR moves them.
  service do
    run [opt_bin/"parcad", "serve"]
    keep_alive true
    log_path var/"log/parcad.log"
    error_log_path var/"log/parcad.log"
  end

  def caveats
    <<~EOS
      Run the host at login, then open http://127.0.0.1:4242 — the whole app
      is there, and an agent's MCP endpoint is http://127.0.0.1:4242/mcp:
        brew services start parcad
      Or just once, in this terminal:
        parcad serve
      Parts are saved in ~/Documents/parcad. Set PARCAD_PROJECTS_DIR to move them.
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/parcad --version")
  end
end
