#!/usr/bin/env python3
"""Compare one Python run and one Rust run of the same cron command on the same seed.

Usage:
  compare.py <command> <mode> <py.json> <rs.json> <py.rc> <rs.rc> <py_rows.json> <rs_rows.json> \
             <py_calls.jsonl> <rs_calls.jsonl> <py_start> <py_finish> <rs_start> <rs_finish>

Both databases are reset from the SAME seeded template before their run, so every timestamp
NOT written by the run is byte-identical between them. A timestamp the run DOES write is
normalized to "<set-by-run>", detected by falling inside that run's [start, finish] window.
The payload's own `now`/`cutoff`/`at` are normalized to "<ts>".

Prints a JSON verdict; exit 0 iff exit codes, payloads, row effects and billing calls match.
Billing requests are compared as a multiset (sorted JSON), because teardown calls are
issued concurrently by both implementations.
"""
from __future__ import annotations

import json
import sys
from datetime import datetime

PAYLOAD_TS_KEYS = {"now", "cutoff", "at"}
ROW_TS_KEYS = {"revoked_at", "updated_at", "metering_verified_at", "last_used_at"}


def load_json(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def load_calls(path):
    try:
        with open(path, encoding="utf-8") as handle:
            calls = [json.loads(line) for line in handle if line.strip()]
    except FileNotFoundError:
        return []
    for call in calls:
        if "query" in call:
            call["query"] = {key: sorted(values) for key, values in sorted(call["query"].items())}
    return sorted(json.dumps(call, sort_keys=True) for call in calls)


def epoch(value):
    if value is None:
        return None
    return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()


def normalize_payload(payload):
    out = dict(payload)
    for key in PAYLOAD_TS_KEYS:
        if key in out:
            out[key] = "<ts>"
    return out


def normalize_rows(rowdoc, start, finish):
    rows = {}
    for table, items in rowdoc.items():
        rows[table] = {}
        for item in items:
            row = dict(item)
            for key in ROW_TS_KEYS:
                if key in row and row[key] is not None:
                    at = epoch(row[key])
                    if at is not None and start - 3 <= at <= finish + 3:
                        row[key] = "<set-by-run>"
            rows[table][row["id"]] = row
    return rows


def diff_maps(py_map, rs_map):
    out = {}
    for key in sorted(set(py_map) | set(rs_map)):
        if py_map.get(key, "<missing>") != rs_map.get(key, "<missing>"):
            out[key] = {"python": py_map.get(key, "<missing>"),
                        "rust": rs_map.get(key, "<missing>")}
    return out


def main() -> int:
    (command, mode, py_json, rs_json, py_rc, rs_rc, py_rows, rs_rows,
     py_calls, rs_calls, py_start, py_finish, rs_start, rs_finish) = sys.argv[1:15]

    py_payload = normalize_payload(load_json(py_json))
    rs_payload = normalize_payload(load_json(rs_json))
    py_code = open(py_rc, encoding="utf-8").read().strip()
    rs_code = open(rs_rc, encoding="utf-8").read().strip()
    payload_diff = diff_maps(py_payload, rs_payload)

    py_rows_norm = normalize_rows(load_json(py_rows), float(py_start), float(py_finish))
    rs_rows_norm = normalize_rows(load_json(rs_rows), float(rs_start), float(rs_finish))
    row_diff = {}
    for table in sorted(set(py_rows_norm) | set(rs_rows_norm)):
        table_diff = diff_maps(py_rows_norm.get(table, {}), rs_rows_norm.get(table, {}))
        if table_diff:
            row_diff[table] = table_diff

    py_call_list, rs_call_list = load_calls(py_calls), load_calls(rs_calls)
    calls_equal = py_call_list == rs_call_list
    equal = py_code == rs_code and not payload_diff and not row_diff and calls_equal

    verdict = {
        "command": command,
        "mode": mode,
        "equal": equal,
        "exit_codes": {"python": int(py_code), "rust": int(rs_code)},
        "payload_diff": payload_diff,
        "row_diff": row_diff,
        "billing_calls_equal": calls_equal,
        "billing_calls": {"python": py_call_list, "rust": rs_call_list},
    }
    print(json.dumps(verdict, indent=2, sort_keys=True))
    return 0 if equal else 1


if __name__ == "__main__":
    raise SystemExit(main())
