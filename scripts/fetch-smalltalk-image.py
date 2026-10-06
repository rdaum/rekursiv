#!/usr/bin/env python3
"""Fetch and verify the pinned Xerox distribution; never execute its contents."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile
import urllib.request


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    repo = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", nargs="?", type=Path, default=repo / "artifacts/st80")
    args = parser.parse_args()
    manifest = json.loads((repo / "crates/rekursiv-smalltalk/fixtures/xerox-v2.json").read_text())
    destination = args.directory
    if all((destination / name).is_file() and digest((destination / name).read_bytes()) == expected
           for name, expected in manifest["files"].items()):
        print(f"Pinned Smalltalk-80 distribution verified: {destination}")
        return

    with urllib.request.urlopen(manifest["archive_url"], timeout=60) as response:
        archive_bytes = response.read(4 * 1024 * 1024)
    if digest(archive_bytes) != manifest["archive_sha256"]:
        raise ValueError("distribution archive SHA-256 mismatch")
    # Read only named files, with no extraction of archive paths or permissions.
    payloads = {}
    with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:gz") as archive:
        for name, expected in manifest["files"].items():
            member = archive.getmember(name)
            if not member.isfile() or member.size > 2 * 1024 * 1024:
                raise ValueError(f"unexpected archive member: {name}")
            with archive.extractfile(member) as stream:
                data = stream.read()
            if digest(data) != expected:
                raise ValueError(f"SHA-256 mismatch: {name}")
            payloads[name] = data
    destination.mkdir(parents=True, exist_ok=True)
    for name, data in payloads.items():
        (destination / name).write_bytes(data)
    print(f"Fetched and verified pinned Smalltalk-80 distribution: {destination}")


if __name__ == "__main__":
    main()
