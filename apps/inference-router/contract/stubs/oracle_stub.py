#!/usr/bin/env python3
"""The oracle's world: one deterministic double for the three services inference-router calls.

Capture-time fixture, not product code. It is deliberately stdlib-only and byte-for-byte
deterministic (no clock, no randomness) so a golden set captured from the old image can be
re-captured and compared.

Roles, by path:
  POST /v1/chat/completions     an OpenAI-compatible upstream (non-stream and SSE)
  POST /license/introspect      the licences introspection endpoint
  POST /api/budget/reserve      billing-service's pre-spend token budget gate

Synthetic identifiers only. No production token, tenant or prompt appears here.
"""

import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# Fixed identifiers: the golden normaliser may still mask them, but a fixture that changes
# between runs makes "reproducible" untestable.
TENANT = "00000000-0000-4000-8000-0000000000a1"
TENANT_OVER_BUDGET = "00000000-0000-4000-8000-0000000000ba"
TENANT_NO_BUDGET = "00000000-0000-4000-8000-0000000000cb"
LICENSE = "00000000-0000-4000-8000-0000000000c1"
RESERVATION = "00000000-0000-4000-8000-0000000000d1"
CREATED = 1700000000
MODEL = "stub-model"

# Tokens the licences stub understands. Anything else is an unknown token, which the live
# licences API answers with 200 {"active": false} — never a transport error.
TOKENS = {
    "stub-licence-token-0000000000000000000001": True,
    "stub-licence-inactive-00000000000000000002": False,
}

USAGE = {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10}
CONTENT = "stub completion"


def completion(request: dict) -> dict:
    return {
        "id": "chatcmpl-stub",
        "object": "chat.completion",
        "created": CREATED,
        "model": request.get("model") or MODEL,
        "choices": [
            {
                "index": 0,
                "message": {"role": "assistant", "content": CONTENT},
                "finish_reason": "stop",
            }
        ],
        "usage": USAGE,
    }


def chunk(delta: dict, finish: str | None) -> dict:
    return {
        "id": "chatcmpl-stub",
        "object": "chat.completion.chunk",
        "created": CREATED,
        "model": MODEL,
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
    }


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):  # keep the container log readable
        sys.stderr.write("stub: " + (fmt % args) + "\n")

    def _json(self, status: int, payload: dict) -> None:
        body = json.dumps(payload, sort_keys=True).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _body(self) -> dict:
        length = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(length) if length else b"{}"
        try:
            parsed = json.loads(raw or b"{}")
        except json.JSONDecodeError:
            return {}
        return parsed if isinstance(parsed, dict) else {}

    def do_GET(self):  # noqa: N802 — BaseHTTPRequestHandler's spelling
        if self.path in ("/__health", "/health"):
            self._json(200, {"status": "ok"})
        else:
            self._json(404, {"detail": "not found"})

    def do_POST(self):  # noqa: N802
        # The body MUST be drained on every path: this server is HTTP/1.1 keep-alive, so an
        # unread body is parsed as the next request line and the following call dies with a
        # 501 naming its own JSON. (Measured: the reserve call left its body and the provider
        # call then read it as a method.)
        request = self._body()
        if self.path == "/v1/chat/completions":
            self.chat(request)
        elif self.path == "/license/introspect":
            self.introspect(request)
        elif self.path == "/api/budget/reserve":
            self.reserve(request)
        else:
            self._json(404, {"detail": "not found"})

    def chat(self, request):
        if request.get("model") == "stub-error" or self.headers.get("x-stub-mode") == "error":
            self._json(500, {"error": {"message": "stub provider failure", "type": "server_error"}})
            return
        if not request.get("stream"):
            self._json(200, completion(request))
            return

        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("cache-control", "no-cache")
        self.send_header("connection", "keep-alive")
        self.end_headers()
        frames = [
            chunk({"role": "assistant", "content": ""}, None),
            chunk({"content": "stub "}, None),
            chunk({"content": "completion"}, None),
            chunk({}, "stop"),
            # The usage-bearing terminal chunk, as OpenAI sends with stream_options.
            {
                "id": "chatcmpl-stub",
                "object": "chat.completion.chunk",
                "created": CREATED,
                "model": MODEL,
                "choices": [],
                "usage": USAGE,
            },
        ]
        for frame in frames:
            self.wfile.write(("data: " + json.dumps(frame, sort_keys=True) + "\n\n").encode())
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()

    def introspect(self, request):
        token = str(request.get("token") or "")
        if TOKENS.get(token) is True:
            self._json(
                200,
                {
                    "active": True,
                    "tenant_id": TENANT,
                    "license_id": LICENSE,
                    "app": {"key": "inference-router"},
                    "expires_at": "2030-01-01T00:00:00Z",
                },
            )
        elif token in TOKENS:  # a known-but-inactive token: 200 with active false
            self._json(200, {"active": False})
        else:
            self._json(200, {"active": False})

    def reserve(self, request):
        tenant = self.headers.get("x-tenant-id") or ""
        if tenant == TENANT_OVER_BUDGET:
            self._json(
                429,
                {
                    "error": {
                        "details": {
                            "reason": "over_budget",
                            "remaining_tokens": 0,
                            "cap_tokens": 1000,
                        }
                    }
                },
            )
        elif tenant == TENANT_NO_BUDGET:
            # A 429 that is NOT a breach: the router must allow and count it.
            self._json(429, {"error": {"details": {"reason": "no_active_budget"}}})
        else:
            self._json(
                200,
                {
                    "data": {
                        "allowed": True,
                        "remaining_tokens": 999999,
                        "cap_tokens": 1000000,
                        "reservation_id": RESERVATION,
                    }
                },
            )


def main() -> None:
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 9100
    server = ThreadingHTTPServer(("0.0.0.0", port), Handler)
    sys.stderr.write(f"stub: listening on :{port}\n")
    server.serve_forever()


if __name__ == "__main__":
    main()
