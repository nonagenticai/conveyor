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
import sys

from private_refs import redact

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
