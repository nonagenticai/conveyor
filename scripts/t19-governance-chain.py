#!/usr/bin/env python3
"""t19-governance-chain — the T19 invocation of `scripts/governance-chain.py`.

T19's live chain (its recorded evidence under `docs/evidence/t19/gov/`) keeps this command, so
the act can be re-run exactly as it was; the engine and the T19 content now live in the generic
script, which carries one profile per project:

    python3 scripts/t19-governance-chain.py --check     # = governance-chain.py --project T19 --check
    python3 scripts/t19-governance-chain.py --apply     # = governance-chain.py --project T19 --apply
"""

from __future__ import annotations

import runpy
import sys
from pathlib import Path

GENERIC = Path(__file__).resolve().with_name("governance-chain.py")
sys.argv = [str(GENERIC), "--project", "T19", *sys.argv[1:]]
runpy.run_path(str(GENERIC), run_name="__main__")
