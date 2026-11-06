# Draft formula for firstprayer/homebrew-tap. No release exists yet: the
# version and checksums below are placeholders. It cannot go in homebrew/core,
# which accepts only open source licenses.
class Deft < Formula
  desc "One command for the file jobs AI agents otherwise script"
  homepage "https://github.com/firstprayer/deft"
  version "0.1.0"
  license :cannot_represent

  on_macos do
    on_arm do
      url "https://github.com/firstprayer/deft/releases/download/v0.1.0/deft-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
    on_intel do
      url "https://github.com/firstprayer/deft/releases/download/v0.1.0/deft-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/firstprayer/deft/releases/download/v0.1.0/deft-x86_64-unknown-linux-musl.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
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
