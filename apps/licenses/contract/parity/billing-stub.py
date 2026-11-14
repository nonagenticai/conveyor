#!/usr/bin/env python3
"""Tiny deterministic stand-in for the billing endpoints the cron jobs call.

State is per-PROCESS: restart it with a fresh log file before each Python/Rust run so both
sides start from the same initial billing facts. Every request is appended as one JSON line
to the log path given as argv[2].

Endpoints:
  GET    /api/budget/list/{tenant}   -> {"data":[active license_sync rows]}
  GET    /api/quota/check            -> {"data":{"quota_monthly": <int|null>}}
  POST   /api/budget                 -> upsert license_sync budget
  POST   /api/quota/config           -> upsert quota config
  DELETE /api/budget/{id}            -> soft delete (active=false)
  DELETE /api/quota/config/{licence} -> delete
"""
from __future__ import annotations

import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

T1 = "11111111-1111-4111-8111-111111111111"
T2 = "11111111-1111-4111-8111-222222222222"

# Initial billing facts. Keep in lockstep with seed.sql.
BUDGETS = [
    {"id": "cccccccc-0000-4000-8000-000000000003", "tenant_id": T1,
     "license_id": "aaaaaaaa-0000-4000-8000-000000000003", "budget_tokens": 500000,
     "period": "monthly", "source": "license_sync", "active": True},
    {"id": "cccccccc-0000-4000-8000-000000000011", "tenant_id": T1,
     "license_id": "aaaaaaaa-0000-4000-8000-000000000011", "budget_tokens": 600000,
     "period": "monthly", "source": "license_sync", "active": True},
    {"id": "cccccccc-0000-4000-8000-000000000005", "tenant_id": T2,
     "license_id": "aaaaaaaa-0000-4000-8000-000000000005", "budget_tokens": 500000,
     "period": "monthly", "source": "license_sync", "active": True},
]
QUOTA = {
    "aaaaaaaa-0000-4000-8000-000000000003": 1000,
    "aaaaaaaa-0000-4000-8000-000000000011": 900,
    "aaaaaaaa-0000-4000-8000-000000000005": 1000,
}
LOCK = threading.RLock()
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8347
LOG_PATH = sys.argv[2] if len(sys.argv) > 2 else "/tmp/parity/out/stub-calls.jsonl"
_seq = [0]


def log(entry: dict) -> None:
    with LOCK:
        with open(LOG_PATH, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(entry, sort_keys=True) + "\n")


class Handler(BaseHTTPRequestHandler):
    def reply(self, code: int, payload: dict) -> None:
        body = json.dumps(payload).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802
        url = urlparse(self.path)
        parts = url.path.strip("/").split("/")
        with LOCK:
            if len(parts) == 4 and parts[:3] == ["api", "budget", "list"]:
                tenant = parts[3]
                log({"method": "GET", "path": url.path,
                     "tenant_header": self.headers.get("X-Tenant-Id")})
                rows = [dict(row) for row in BUDGETS if row["tenant_id"] == tenant and row["active"]]
                return self.reply(200, {"data": rows})
            if url.path == "/api/quota/check":
                query = parse_qs(url.query)
                licence = (query.get("license_id") or [None])[0]
                log({"method": "GET", "path": url.path, "query": query,
                     "tenant_header": self.headers.get("X-Tenant-Id")})
                return self.reply(200, {"data": {"quota_monthly": QUOTA.get(licence)}})
        return self.reply(404, {"detail": "not found"})

    def do_POST(self) -> None:  # noqa: N802
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b""
        try:
            payload = json.loads(raw or b"{}")
        except Exception:
            payload = {"unparseable": raw.decode("utf-8", "replace")}
        with LOCK:
            log({"method": "POST", "path": self.path,
                 "tenant_header": self.headers.get("X-Tenant-Id"), "body": payload})
            if self.path == "/api/budget":
                licence = payload.get("license_id")
                for row in BUDGETS:
                    if row["license_id"] == licence and row["active"]:
                        row["budget_tokens"] = payload.get("budget_tokens")
                        break
                else:
                    _seq[0] += 1
                    BUDGETS.append({
                        "id": f"cccccccc-0000-4000-8000-{_seq[0]:012d}",
                        "tenant_id": payload.get("tenant_id"),
                        "license_id": licence,
                        "budget_tokens": payload.get("budget_tokens"),
                        "period": payload.get("period", "monthly"),
                        "source": payload.get("source", "license_sync"),
                        "active": True,
                    })
                return self.reply(201, {"data": {"ok": True}})
            if self.path == "/api/quota/config":
                QUOTA[payload.get("license_id")] = payload.get("quota_monthly")
                return self.reply(201, {"data": {"ok": True}})
        return self.reply(404, {"detail": "not found"})

    def do_DELETE(self) -> None:  # noqa: N802
        parts = self.path.strip("/").split("/")
        with LOCK:
            log({"method": "DELETE", "path": self.path,
                 "tenant_header": self.headers.get("X-Tenant-Id")})
            if len(parts) == 3 and parts[:2] == ["api", "budget"]:
                budget_id = parts[2]
                for row in BUDGETS:
                    if row["id"] == budget_id:
                        row["active"] = False
                return self.reply(200, {"ok": True})
            if len(parts) == 4 and parts[:3] == ["api", "quota", "config"]:
                QUOTA.pop(parts[3], None)
                return self.reply(200, {"ok": True})
        return self.reply(404, {"detail": "not found"})

    def log_message(self, *_args) -> None:
        pass


if __name__ == "__main__":
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
