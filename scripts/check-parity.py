#!/usr/bin/env python3
"""Validate the signed S00 parity contract and its native acceptance map.

The contract is `parity/parity.yaml` (1,300 rows). Its signature is the sha256
recorded in `parity/SUMMARY.md`; a change to the contract without a re-sign is a
failure, and so is a schema, id, count or acceptance-form violation.

P2's `parity/native.tsv` maps every `used` id to a native disposition; the CI job passes
`--require-native`, so the map cannot silently lose a row.

Usage: check-parity.py [--require-native] [--self-test]
"""

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONTRACT = ROOT / "parity" / "parity.yaml"
SCHEMA = ROOT / "parity" / "schema.json"
SUMMARY = ROOT / "parity" / "SUMMARY.md"
NATIVE = ROOT / "parity" / "native.tsv"

SIGNED_COUNTS = {"used": 218, "rare": 295, "unused": 543, "dropped": 244}
ACCEPTANCE_FORM = re.compile(r"^(test|e2e|probe):\S")
NATIVE_FORM = re.compile(r"^(action|test|replacement|dropped):\S")
ID_FORM = re.compile(r"^P-[0-9]{4}$")


def signed_sha(summary_text):
    match = re.search(r"sha256:\s*([0-9a-f]{64})", summary_text)
    return match.group(1) if match else None


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def validate(rows, schema, recorded_sha, contract_sha, native_rows=None, require_native=False):
    errors = []
    properties = schema["items"]["properties"]
    required = schema["items"]["required"]
    allowed_keys = set(properties)
    status_enum = properties["status"]["enum"]
    app_enum = properties["app"]["enum"]
    class_enum = properties["evidence_class"]["enum"]

    if recorded_sha is None:
        errors.append("SUMMARY.md carries no `sha256:` signature")
    elif recorded_sha != contract_sha:
        errors.append(
            f"the contract hash {contract_sha} does not match the signed {recorded_sha} "
            "(re-sign in SUMMARY.md after an owner-approved change)"
        )

    if not isinstance(rows, list):
        return errors + ["parity.yaml is not a list"]

    seen = set()
    counts = {name: 0 for name in status_enum}
    for index, row in enumerate(rows):
        where = f"row {index}"
        if not isinstance(row, dict):
            errors.append(f"{where}: not an object")
            continue
        missing = [key for key in required if key not in row]
        if missing:
            errors.append(f"{where}: missing {', '.join(missing)}")
            continue
        extra = [key for key in row if key not in allowed_keys]
        if extra:
            errors.append(f"{where}: unknown fields {', '.join(extra)}")
        row_id = row["id"]
        if not isinstance(row_id, str) or not ID_FORM.match(row_id):
            errors.append(f"{where}: bad id {row_id!r}")
        elif row_id in seen:
            errors.append(f"{where}: duplicate id {row_id}")
        else:
            seen.add(row_id)
        for key in required:
            if not isinstance(row[key], str):
                errors.append(f"{where}: {key} is not a string")
        if row.get("status") not in status_enum:
            errors.append(f"{where}: bad status {row.get('status')!r}")
        else:
            counts[row["status"]] += 1
        if row.get("app") not in app_enum:
            errors.append(f"{where}: bad app {row.get('app')!r}")
        if row.get("evidence_class") not in class_enum:
            errors.append(f"{where}: bad evidence_class {row.get('evidence_class')!r}")
        if row.get("status") == "used" and not ACCEPTANCE_FORM.match(row.get("acceptance", "")):
            errors.append(f"{where}: used row {row_id} has no test/e2e/probe acceptance")

    if counts != SIGNED_COUNTS:
        errors.append(f"status counts {counts} do not match the signed {SIGNED_COUNTS}")
    if len(rows) != sum(SIGNED_COUNTS.values()):
        errors.append(f"row count {len(rows)} does not match the signed 1300")

    if require_native:
        if native_rows is None:
            errors.append("parity/native.tsv is required but absent")
        else:
            native_ids = {row["id"] for row in native_rows}
            used_ids = {row["id"] for row in rows if row.get("status") == "used"}
            missing = used_ids - native_ids
            unknown = native_ids - used_ids
            if missing:
                errors.append(f"native.tsv is missing {len(missing)} used id(s): {sorted(missing)[:5]}")
            if unknown:
                errors.append(f"native.tsv names {len(unknown)} non-used id(s): {sorted(unknown)[:5]}")
            for row in native_rows:
                if not NATIVE_FORM.match(row.get("native", "")):
                    errors.append(f"native.tsv {row.get('id')}: bad disposition {row.get('native')!r}")
    return errors


def load_native(path):
    if not path.is_file():
        return None
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line and not line.startswith("#")]
    header = lines[0].split("\t")
    if header != ["id", "surface", "native", "reference"]:
        raise SystemExit(f"parity: FAIL: native.tsv header is {header}, expected id/surface/native/reference")
    rows = []
    for line in lines[1:]:
        fields = line.split("\t")
        if len(fields) != 4:
            raise SystemExit(f"parity: FAIL: native.tsv row has {len(fields)} fields: {line[:80]}")
        rows.append(dict(zip(header, fields)))
    return rows


def main(argv):
    contract_bytes = CONTRACT.read_bytes()
    contract_sha = hashlib.sha256(contract_bytes).hexdigest()
    rows = json.loads(contract_bytes)
    schema = load(SCHEMA)
    recorded = signed_sha(SUMMARY.read_text(encoding="utf-8"))
    native_rows = load_native(NATIVE)
    require_native = "--require-native" in argv or native_rows is not None

    if "--self-test" in argv:
        failures = 0
        base = validate(rows, schema, contract_sha, contract_sha)
        if base:
            print(f"parity self-test: baseline unexpectedly fails: {base[:3]}"); failures += 1
        bad_status = [dict(row) for row in rows]
        bad_status[0]["status"] = "maybe"
        if not any("bad status" in e for e in validate(bad_status, schema, contract_sha, contract_sha)):
            print("parity self-test: bad status not caught"); failures += 1
        duplicate = [dict(row) for row in rows]
        duplicate[1]["id"] = duplicate[0]["id"]
        if not any("duplicate id" in e for e in validate(duplicate, schema, contract_sha, contract_sha)):
            print("parity self-test: duplicate id not caught"); failures += 1
        if not any("does not match" in e for e in validate(rows, schema, "0" * 64, contract_sha)):
            print("parity self-test: stale signature not caught"); failures += 1
        print(f"parity self-test: {'PASS' if failures == 0 else 'FAIL'}")
        return 1 if failures else 0

    errors = validate(rows, schema, recorded, contract_sha, native_rows, require_native)
    if errors:
        print(f"parity: FAIL: {len(errors)} problem(s)")
        for error in errors:
            print(f"  - {error}")
        return 1
    counts = ", ".join(f"{name} {SIGNED_COUNTS[name]}" for name in ("used", "rare", "unused", "dropped"))
    native = "native map present" if native_rows is not None else "native map not yet present (P2)"
    print(f"parity: OK: {contract_sha[:12]} signed, 1300 rows ({counts}); {native}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
