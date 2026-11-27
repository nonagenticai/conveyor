#!/usr/bin/env python3
"""The passing-subset control and one certificate re-verification (conveyor#855).

    control_units.py control  <run_dir> <url> <token_file> <out_dir>
    control_units.py reverify <run_dir> <url> <token_file> <proof_id> <chunk>

`control` keeps only the units that pass every rule in the collector's local mirror, sorts
them by key, chunks at the collector's `CHUNK_SIZE`, writes the facts under a derived corpus
hash, and runs the collector's own `certify` over them. It is a control: it proves the live
record path accepts a real, proven set; it is never part of the evaluation totals.

`reverify` re-renders a chunk's facts (deterministic), runs a fresh `lean4.check` under a key
that differs from the certificate's own, and asks `proofs.reverify` to compare it against the
stored row. It prints the fresh verdict and the stored certificate as JSON.

The token is a tenant licence bearer (`conveyor auth token --tenant <slug>`); nothing here
prints it. See `docs/evidence/proof-of-delivery.md` § Evaluation for the measurement.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import collect_units as cu  # noqa: E402


def control(run_dir: pathlib.Path, url: str, token_file: pathlib.Path, out_dir: pathlib.Path) -> int:
    corpus = json.loads((run_dir / "units.json").read_text())
    passing = [unit for unit in corpus["units"] if all(cu.local_rules(unit["facts"]).values())]
    passing.sort(key=lambda unit: unit["key"])
    control_sha = hashlib.sha256((corpus["corpus_sha256"] + ":passing-control").encode()).hexdigest()
    control_corpus = {**corpus, "corpus_sha256": control_sha, "units": passing}
    parts = cu.chunks(passing, cu.CHUNK_SIZE)
    control_corpus["chunks"] = [
        {"index": index, "name": f"chunk-{index:03d}", "units": len(chunk)}
        for index, chunk in enumerate(parts)
    ]

    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "units.json").write_text(
        json.dumps(control_corpus, indent=2, sort_keys=True) + "\n"
    )
    facts = out_dir / "facts"
    facts.mkdir(exist_ok=True)
    for index, chunk in enumerate(parts):
        document = cu.facts_document(control_corpus, chunk)
        (facts / f"chunk-{index:03d}.json").write_text(
            json.dumps(document, indent=2, sort_keys=True) + "\n"
        )

    print(
        f"control corpus {control_sha[:16]}…: {len(passing)} passing units, {len(parts)} chunk(s)"
    )
    cu.certify(out_dir, url, token_file)
    return 0


def reverify(
    run_dir: pathlib.Path,
    url: str,
    token_file: pathlib.Path,
    proof_id: str,
    chunk: str,
) -> int:
    facts = json.loads((run_dir / "facts" / f"{chunk}.json").read_text())
    facts_text = cu.canonical(facts)
    engine = cu.Engine(url, cu.token_from_file(token_file))
    key = f"unit-ledger:recheck:{proof_id}:{int(time.time())}"

    rendered = engine.call(
        "proofs.render",
        {"facts_canonical": facts_text, "theorem": "unit_ledger_ok"},
        f"{key}:render",
    )["output"]
    check_input = {"source": rendered["lean_source"], "timeout": 120}
    checked = engine.call("lean4.check", check_input, f"{key}:check")["output"]
    reverified = engine.call(
        "proofs.reverify",
        {
            "proof_id": proof_id,
            "action": "lean4.check",
            "idempotency_key": f"{key}:check",
            "invocation_input": check_input,
        },
        f"{key}:reverify",
    )["output"]

    print(
        json.dumps(
            {
                "proof_id": proof_id,
                "chunk": chunk,
                "fresh_check_valid": checked.get("valid"),
                "fresh_check_ms": checked.get("computation_time_ms"),
                "facts_sha256": rendered["facts_sha256"],
                "model_sha256": rendered["model_sha256"],
                "spec_sha256": rendered["spec_sha256"],
                "reverified": reverified,
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)

    control_parser = sub.add_parser("control", help="certify the passing subset as a control")
    control_parser.add_argument("run", type=pathlib.Path)
    control_parser.add_argument("url")
    control_parser.add_argument("token_file", type=pathlib.Path)
    control_parser.add_argument("out", type=pathlib.Path)

    reverify_parser = sub.add_parser("reverify", help="re-verify one stored certificate")
    reverify_parser.add_argument("run", type=pathlib.Path)
    reverify_parser.add_argument("url")
    reverify_parser.add_argument("token_file", type=pathlib.Path)
    reverify_parser.add_argument("proof_id")
    reverify_parser.add_argument("chunk")

    args = parser.parse_args()
    if args.mode == "control":
        return control(args.run, args.url, args.token_file, args.out)
    return reverify(args.run, args.url, args.token_file, args.proof_id, args.chunk)


if __name__ == "__main__":
    raise SystemExit(main())
