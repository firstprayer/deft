# The formula for firstprayer/homebrew-tap. It cannot go in homebrew/core,
# which accepts only open source licenses.
class Deft < Formula
  desc "One command for the file jobs AI agents otherwise script"
  homepage "https://deftcli.com"
  version "0.2.0"
  license :cannot_represent

  on_macos do
    on_arm do
      url "https://github.com/firstprayer/deft/releases/download/v0.2.0/deft-aarch64-apple-darwin.tar.gz"
      sha256 "5a4f825bbb20d599292855c7e3bf15feff8966c2ea2ecdb7ea868a3f78f68467"
    end
    on_intel do
      url "https://github.com/firstprayer/deft/releases/download/v0.2.0/deft-x86_64-apple-darwin.tar.gz"
      sha256 "9be003a8d52c0920add22f5da6daeae0f7f61db6bca25147f866e43805e374a4"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/firstprayer/deft/releases/download/v0.2.0/deft-aarch64-unknown-linux-musl.tar.gz"
      sha256 "a445df8612820c36e42405e76f8f169c903c91c1b926217e5fe367c45df573c3"
    end
    on_intel do
      url "https://github.com/firstprayer/deft/releases/download/v0.2.0/deft-x86_64-unknown-linux-musl.tar.gz"
      sha256 "5143239dc856ac79c74d72875e254901d999fec220923823c38b229ced8c1f91"
    end
  end

  def install
    bin.install "deft"
    doc.install "LICENSE.md", "README.md", "THIRD-PARTY-NOTICES.md"
  end

  def caveats
    <<~EOS
      HEIC and AVIF images need ffmpeg:      brew install ffmpeg
      OCR needs tesseract:                   brew install tesseract
      Check what is available with:          deft doctor
      Teach your agents to use deft with:    deft skill install --agent all
    EOS
  end

  test do
    assert_match "deft", shell_output("#{bin}/deft version")
  end
end
