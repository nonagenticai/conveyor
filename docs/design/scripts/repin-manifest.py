#!/usr/bin/env python3
"""Re-pin the design-snapshot manifest after the committed data changed.

The live publish (`conveyor snapshot`) writes `docs/design/data/manifest.json` as an index of
every snapshot file with its sha256, byte length and record count. The automated registry
refresh regenerates some of those files (`issues.json`, `coverage.*`, `standards.*`) without a
database, so it cannot rewrite the index the way the live publish does. Without this step the
committed index describes the previous bytes and `design-pack`'s `check-data.cjs` fails
(measured on the generated PR #907 after #846 expanded the index to every file).

This script attests the files on disk; it never adds, removes or reorders an entry and never
touches the snapshot's provenance fields (`schema`, `git_sha`, `generated_at`, `migration`,
`redaction`, `tenant`, `feedback_url`) or the informational `records` count, which the live
publish derives from its own in-memory values. The next live publish overwrites the manifest.

Run:
  docs/design/scripts/repin-manifest.py           # write the re-pinned index if it drifted
  docs/design/scripts/repin-manifest.py --check   # exit 1 if the index is stale; write nothing
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DEFAULT_DATA_DIR = ROOT / "docs" / "design" / "data"


def rehash(entry: dict, data_dir: Path, root: Path) -> dict:
    """Return the entry with sha256/bytes recomputed from the file on disk."""
    target = (data_dir / entry["path"]).resolve()
    try:
        target.relative_to(root)
    except ValueError:
        raise SystemExit(f"repin-manifest: {entry['path']} escapes the repository")
    if not target.is_file():
        raise SystemExit(f"repin-manifest: {entry['path']} is listed but absent at {target}")
    raw = target.read_bytes()
    pinned = dict(entry)
    pinned["sha256"] = hashlib.sha256(raw).hexdigest()
    pinned["bytes"] = len(raw)
    return pinned


def serialise(manifest: dict) -> bytes:
    """Byte-identical to the Rust writer: serde_json pretty, sorted keys, trailing newline."""
    return (json.dumps(manifest, indent=2, sort_keys=True, ensure_ascii=False) + "\n").encode()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--data-dir",
        type=Path,
        default=DEFAULT_DATA_DIR,
        help="directory holding manifest.json (default: docs/design/data)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="report a stale manifest and exit 1 instead of writing it",
    )
    args = parser.parse_args()

    data_dir = args.data_dir.resolve()
    manifest_path = data_dir / "manifest.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        print(f"repin-manifest: no manifest at {manifest_path}", file=sys.stderr)
        return 1
    except json.JSONDecodeError as error:
        print(f"repin-manifest: {manifest_path} does not parse: {error}", file=sys.stderr)
        return 1

    entries = manifest.get("files")
    if not isinstance(entries, list) or not entries:
        print("repin-manifest: manifest.files is empty", file=sys.stderr)
        return 1

    changed = []
    for index, entry in enumerate(entries):
        pinned = rehash(entry, data_dir, ROOT)
        if pinned != entry:
            changed.append((entry.get("path", f"#{index}"), entry, pinned))
            entries[index] = pinned

    current = serialise(manifest)
    stale = changed or current != manifest_path.read_bytes()

    print(
        f"repin-manifest: {len(entries)} entries, {len(changed)} changed"
        + ("" if not stale else " (index stale)")
    )
    for path, before, after in changed:
        print(
            f"  {path}: {before.get('sha256', '?')[:12]} -> {after['sha256'][:12]}"
            f" ({after['bytes']} bytes, {after['records']} records)"
        )

    if args.check:
        if stale:
            print(
                "repin-manifest: run docs/design/scripts/repin-manifest.py to re-pin",
                file=sys.stderr,
            )
            return 1
        return 0

    if stale:
        manifest_path.write_bytes(current)
    return 0


if __name__ == "__main__":
    sys.exit(main())
