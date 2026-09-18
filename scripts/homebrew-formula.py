#!/usr/bin/env python3
"""Generate the Homebrew formula from release checksums.

Usage:
    python3 scripts/homebrew-formula.py VERSION SHA256SUMS > Formula/onchain.rb
"""

from __future__ import annotations

import argparse
from pathlib import Path, PurePosixPath, PureWindowsPath
import re
import sys
from typing import NoReturn


RELEASE_RE = re.compile(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)")
TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Emit Formula/onchain.rb for a published onchain release."
    )
    parser.add_argument("version", help="release version without the v prefix (for example 0.2.0)")
    parser.add_argument("checksums", type=Path, help="path to the release SHA256SUMS file")
    return parser.parse_args()


def fail(message: str) -> NoReturn:
    raise SystemExit(f"error: {message}")


def expected_archives(version: str) -> dict[str, str]:
    return {
        target: f"onchain-v{version}-{target}.tar.gz"
        for target in TARGETS
    }


def read_checksums(checksums_path: Path, version: str) -> dict[str, str]:
    try:
        lines = checksums_path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError) as error:
        fail(f"cannot read {checksums_path}: {error}")

    checksums: dict[str, str] = {}
    for line_number, raw_line in enumerate(lines, start=1):
        line = raw_line.strip()
        if not line:
            continue

        match = re.fullmatch(r"([0-9A-Fa-f]{64})[ \t]+\*?(\S+)", line)
        if match is None:
            fail(f"invalid SHA256SUMS entry on line {line_number}")

        digest, archive = match.groups()
        if PurePosixPath(archive).name != archive or PureWindowsPath(archive).name != archive:
            fail(f"archive entry must be a basename on line {line_number}: {archive}")
        if archive in checksums:
            fail(f"duplicate archive entry on line {line_number}: {archive}")
        checksums[archive] = digest.lower()

    expected = set(expected_archives(version).values())
    actual = set(checksums)
    if actual != expected:
        details = []
        if missing := sorted(expected - actual):
            details.append(f"missing: {', '.join(missing)}")
        if unexpected := sorted(actual - expected):
            details.append(f"unexpected: {', '.join(unexpected)}")
        fail("SHA256SUMS must contain exactly the four release archives (" + "; ".join(details) + ")")

    return checksums


def render_formula(version: str, checksums: dict[str, str]) -> str:
    archives = expected_archives(version)

    def release(target: str) -> tuple[str, str]:
        archive = archives[target]
        url = f"https://github.com/paperfoot/onchain-cli/releases/download/v{version}/{archive}"
        return url, checksums[archive]

    mac_arm_url, mac_arm_sha = release("aarch64-apple-darwin")
    mac_intel_url, mac_intel_sha = release("x86_64-apple-darwin")
    linux_arm_url, linux_arm_sha = release("aarch64-unknown-linux-gnu")
    linux_intel_url, linux_intel_sha = release("x86_64-unknown-linux-gnu")

    return f'''class Onchain < Formula
  desc "Fast EVM and Zcash queries, transaction investigation, and swap quotes"
  homepage "https://github.com/paperfoot/onchain-cli"
  version "{version}"
  license "MIT"

  on_macos do
    on_arm do
      url "{mac_arm_url}"
      sha256 "{mac_arm_sha}"
    end

    on_intel do
      url "{mac_intel_url}"
      sha256 "{mac_intel_sha}"
    end
  end

  on_linux do
    on_arm do
      url "{linux_arm_url}"
      sha256 "{linux_arm_sha}"
    end

    on_intel do
      url "{linux_intel_url}"
      sha256 "{linux_intel_sha}"
    end
  end

  def install
    bin.install "onchain"
    prefix.install "LICENSE", "README.md"
  end

  test do
    require "json"

    assert_equal "onchain #{{version}}", shell_output("#{{bin}}/onchain --version").strip
    amount = JSON.parse(shell_output("#{{bin}}/onchain --json zcash amount 0.00000001"))
    assert_equal "1", amount.dig("result", "zatoshis")
  end
end
'''


def main() -> None:
    args = parse_args()
    if RELEASE_RE.fullmatch(args.version) is None:
        fail("version must be a simple release version in x.y.z form")

    checksums = read_checksums(args.checksums, args.version)
    sys.stdout.write(render_formula(args.version, checksums))


if __name__ == "__main__":
    main()
