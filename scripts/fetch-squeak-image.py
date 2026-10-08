#!/usr/bin/env python3
"""Fetch the pinned Squeak 1.1 image and source files without running the archive."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import urllib.request
import zipfile


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    repo = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", nargs="?", type=Path, default=repo / "artifacts/squeak-1.1")
    args = parser.parse_args()
    manifest = json.loads((repo / "crates/rekursiv-smalltalk/fixtures/squeak-1.1.json").read_text())
    if all((args.directory / name).is_file()
           and digest((args.directory / name).read_bytes()) == expected
           for name, expected in manifest["files"].items()):
        print(f"Pinned Squeak 1.1 distribution verified: {args.directory}")
        return
    with urllib.request.urlopen(manifest["archive_url"], timeout=60) as response:
        data = response.read(4 * 1024 * 1024)
    if digest(data) != manifest["archive_sha256"]:
        raise ValueError("Squeak archive SHA-256 mismatch")
    payloads = {}
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        for name, expected in manifest["files"].items():
            member = archive.getinfo(name)
            if member.is_dir() or member.file_size > 4 * 1024 * 1024:
                raise ValueError(f"unexpected archive member: {name}")
            payload = archive.read(member)
            if digest(payload) != expected:
                raise ValueError(f"SHA-256 mismatch: {name}")
            payloads[name] = payload
    args.directory.mkdir(parents=True, exist_ok=True)
    for name, data in payloads.items():
        (args.directory / name).write_bytes(data)
    print(f"Fetched and verified pinned Squeak 1.1 distribution: {args.directory}")


if __name__ == "__main__":
    main()
