#!/usr/bin/env python3
"""Redact private infrastructure facts from a register that becomes public.

The rework register's evidence quotes code, plans and cluster records. The pack serves that
register as `data/rework.js` and the live publish embeds it in `snapshot.js`, so a private
address, an internal hostname or a tailnet name must not survive the copy. The redaction is
applied once, at the point every consumer reads, so the committed register, the console global
and the published snapshot are one document.

Usage:
    python3 scripts/redact-public-register.py <register.json>
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

PATTERNS = (
    (re.compile(r"\b(?:\d{1,3}\.){3}\d{1,3}\b"), "[redacted-ip]"),
    (re.compile(r"\b[A-Za-z0-9][A-Za-z0-9_-]*\.internal\b"), "[redacted-host]"),
    (re.compile(r"\bstrato-(?:vps|server)\b"), "[redacted-host]"),
    (re.compile(r"\bts\.net\b"), "tailnet"),
)


def redact(value):
    if isinstance(value, str):
        for pattern, replacement in PATTERNS:
            value = pattern.sub(replacement, value)
        return value
    if isinstance(value, list):
        return [redact(item) for item in value]
    if isinstance(value, dict):
        return {key: redact(item) for key, item in value.items()}
    return value


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: redact-public-register.py <register.json>", file=sys.stderr)
        return 2
    path = pathlib.Path(sys.argv[1])
    register = json.loads(path.read_text(encoding="utf-8"))
    path.write_text(
        json.dumps(redact(register), indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
