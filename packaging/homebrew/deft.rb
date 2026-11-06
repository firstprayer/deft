# The formula for firstprayer/homebrew-tap. It cannot go in homebrew/core,
# which accepts only open source licenses.
class Deft < Formula
  desc "One command for the file jobs AI agents otherwise script"
  homepage "https://deftcli.com"
  version "0.1.0"
  license :cannot_represent

  on_macos do
    on_arm do
      url "https://github.com/firstprayer/deft/releases/download/v0.1.0/deft-aarch64-apple-darwin.tar.gz"
      sha256 "e7955438e2e59dd79fe3b3db5ca29851c1a3faf1f0d048a64cdc2509c9bdf659"
    end
    on_intel do
      url "https://github.com/firstprayer/deft/releases/download/v0.1.0/deft-x86_64-apple-darwin.tar.gz"
      sha256 "ad08d5a3e661eef13c8ce698aa2fe3fc5f724abd97c7d1947b484a53cd68b266"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/firstprayer/deft/releases/download/v0.1.0/deft-aarch64-unknown-linux-musl.tar.gz"
      sha256 "e9768fe395494622c36004dc509d7952e91050dd72753eb3a20bd813fc5e482a"
    end
    on_intel do
      url "https://github.com/firstprayer/deft/releases/download/v0.1.0/deft-x86_64-unknown-linux-musl.tar.gz"
      sha256 "61eead775aabb33376b059ba9f452c106695d85a1c9062b28202fe502aba6b54"
    end
  end

  def install
    bin.install "deft"
    doc.install "LICENSE.md", "README.md", "THIRD-PARTY-NOTICES.md"
  end

  def caveats
    <<~EOS
      Video and audio commands need ffmpeg:  brew install ffmpeg
      Check what is available with:          deft doctor
      Teach your agents to use deft with:    deft skill install --agent all
    EOS
  end

  test do
    assert_match "deft", shell_output("#{bin}/deft version")
  end
end
