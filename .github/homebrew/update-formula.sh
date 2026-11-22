#!/usr/bin/env bash
# Write the Homebrew formula for a published release, from the checksums the
# release workflow attached to it. Run by the `update-tap` job in release.yml;
# runs the same locally, given `gh` logged in.
#
# Usage: update-formula.sh <tag> <formula-file>
#   e.g. update-formula.sh v3.1.0 ../homebrew-tap/Formula/git-tailor.rb
set -euo pipefail

if [ $# -ne 2 ]; then
    echo "usage: $0 <tag> <formula-file>" >&2
    exit 2
fi
tag=$1
out=$2
repo=jordfras/git-tailor
url=https://github.com/$repo/releases/download/$tag

dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
gh release download "$tag" --repo "$repo" --pattern '*.sha256' --dir "$dir"

# Each file reads "<hash>  <archive>".
sha() { cut -d' ' -f1 "$dir/gt-$tag-$1.sha256"; }
macos=$(sha universal-apple-darwin)
linux_arm=$(sha aarch64-unknown-linux-musl)
linux_intel=$(sha x86_64-unknown-linux-musl)

mkdir -p "$(dirname "$out")"
cat >"$out" <<RUBY
# Written by git-tailor's release workflow on each release; edits made here are
# overwritten by the next one.
class GitTailor < Formula
  desc "Interactive terminal tool for tidying up Git commits on a branch"
  homepage "https://github.com/$repo"
  license "Apache-2.0"

  # The universal archive runs on both Apple chips.
  if OS.mac?
    url "$url/gt-$tag-universal-apple-darwin.tar.gz"
    sha256 "$macos"
  elsif Hardware::CPU.arm?
    url "$url/gt-$tag-aarch64-unknown-linux-musl.tar.gz"
    sha256 "$linux_arm"
  else
    url "$url/gt-$tag-x86_64-unknown-linux-musl.tar.gz"
    sha256 "$linux_intel"
  end

  def install
    bin.install "gt"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gt --version")
  end
end
RUBY
