#!/usr/bin/env python3
"""Safely switch only Ubuntu main archive deb822 stanzas to the fallback mirror."""
import argparse
import pathlib
import re

FALLBACK = "http://archive.ubuntu.com/ubuntu"
SOURCE_URI = re.compile(r"^(mirror\+file:/etc/apt/apt-mirrors\.txt|https?://[a-zA-Z0-9.-]*\.archive\.ubuntu\.com/ubuntu)$")


def rewrite(source: str) -> str:
    blocks = re.split(r"(\n\s*\n)", source)
    changed = 0
    for i in range(0, len(blocks), 2):
        block = blocks[i]
        suites = re.search(r"(?m)^Suites:\s*(.+)$", block)
        uri = re.search(r"(?m)^(URIs:\s*)(\S+)(\s*)$", block)
        if not suites or not uri:
            continue
        names = suites.group(1).split()
        if "noble-security" in names:
            continue
        if not names or any(n not in ("noble", "noble-updates", "noble-backports") for n in names):
            continue
        if not SOURCE_URI.fullmatch(uri.group(2)):
            continue
        blocks[i] = block[:uri.start(2)] + FALLBACK + block[uri.end(2):]
        changed += 1
    if changed == 0:
        raise ValueError("No supported Ubuntu main-archive stanza found; refusing to change APT sources")
    return "".join(blocks)


def self_test() -> None:
    shared = "mirror+file:/etc/apt/apt-mirrors.txt"
    original = (
        f"Types: deb\nURIs: {shared}\nSuites: noble noble-updates noble-backports\n"
        "Components: main universe\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n\n"
        f"Types: deb\nURIs: {shared}\nSuites: noble-security\n"
        "Components: main universe\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n"
    )
    updated = rewrite(original)
    assert updated.count(FALLBACK) == 1
    assert updated.split("\n\n")[1] == original.split("\n\n")[1]
    assert "Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg" in updated
    try:
        rewrite("Types: deb\nURIs: " + shared + "\nSuites: noble-security\n")
    except ValueError:
        pass
    else:
        raise AssertionError("Security-only sources must not be rewritten")
    print("APT fallback source isolation self-test passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("source", nargs="?")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        if not args.source:
            parser.error("source path is required")
        path = pathlib.Path(args.source)
        path.write_text(rewrite(path.read_text()))
