#!/usr/bin/env python3
"""The egress allowlist as an executable instrument (S09 property f).

SPEC_V2's S09 gate claims "zero `reqwest::` outside the executor". The claim was
proved by walking one directory and allowlisting transports beside it, so direct
HTTP in `crates/auth`, `crates/api` or `apps/server` was never seen. This script
states the property at its true scope — the WHOLE cargo workspace — and enforces an
exact allowlist: every tracked Rust source file that mentions `reqwest::` must be
named here with the role that justifies it.

Modes:
  --check       scan the workspace; an unlisted offender or a stale entry exits 1
  --self-test   build a fixture with one allowed file and one planted offender and
                assert the scan refuses the offender (no workspace needed)

Dependency-free (standard library only).
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MARKER = "reqwest::"

#: Exact allowlist: the only tracked source files that may reference `reqwest::`, each with
#: the role that makes it a declared transport rather than an ad-hoc call. A file not named
#: here fails the scan; an entry whose file no longer references the marker is stale and also
#: fails, so the list can never grow silently or rot.
ALLOW: dict[str, str] = {
    # The engine: the one action path that calls an external engine, with timeout, breaker,
    # idempotency and audit (docs/plans/slices/S09-documents-integrations.md, property f).
    "crates/actions/src/invoke.rs": "engine",
    "crates/actions/src/produce.rs": "declared transport: streaming producer",
    "crates/actions/src/tool_authoring.rs": "declared transport: tool authoring probes",
    "crates/actions/src/toolkits/probe.rs": "declared transport: toolkit probe",
    "crates/actions/src/board/webhooks.rs": "declared transport: board webhook delivery",
    "crates/actions/src/channel/connectors/webhook.rs": "declared transport: channel webhook",
    "crates/actions/src/control/channels.rs": "declared transport: channel control",
    # The integrations are the product's declared external clients; each carries its own
    # timeout and is exercised through the engine's audit path.
    "crates/actions/src/integrations/audio.rs": "declared transport: integration",
    "crates/actions/src/integrations/convert.rs": "declared transport: integration",
    "crates/actions/src/integrations/external.rs": "declared transport: integration",
    "crates/actions/src/integrations/github.rs": "declared transport: integration",
    "crates/actions/src/integrations/imagegen.rs": "declared transport: integration",
    "crates/actions/src/integrations/inference.rs": "declared transport: integration",
    "crates/actions/src/integrations/ingest.rs": "declared transport: integration",
    "crates/actions/src/integrations/strata.rs": "declared transport: integration",
    "crates/actions/src/integrations/youtube.rs": "declared transport: integration",
    # Identity-provider and licence calls: Keycloak discovery/JWKS, machine tokens, licence
    # introspection. These are authentication, not product egress through the engine.
    "crates/auth/src/jwks.rs": "declared transport: identity provider",
    "crates/auth/src/licence.rs": "declared transport: licence service",
    "crates/auth/src/machine.rs": "declared transport: identity provider",
    "crates/auth/src/oauth.rs": "declared transport: identity provider",
    "crates/auth/src/service.rs": "declared transport: identity provider",
    "crates/auth/src/url.rs": "declared transport: identity provider",
    # Operations probes (read-only CI and health queries).
    "crates/ops/src/ci.rs": "declared transport: ops probe",
    "crates/ops/src/lib.rs": "declared transport: ops probe",
    # The runner's cluster control-plane client.
    "crates/runner/src/cluster.rs": "declared transport: runner cluster",
    # Each app under apps/ is its own deployable service with its own clients; they are not
    # the conveyor server's egress and each is gated by its own service's checks.
    "apps/billing-shim/src/upstream.rs": "service client: billing-shim",
    "apps/inference-router/src/auth.rs": "service client: inference-router",
    "apps/inference-router/src/billing.rs": "service client: inference-router",
    "apps/inference-router/src/provider/openai.rs": "service client: inference-router",
    "apps/lean4-prover/src/auth.rs": "service client: lean4-prover",
    "apps/licenses/src/billing/metering.rs": "service client: licenses",
    "apps/media/src/common/runner.rs": "service client: media",
    "apps/research/src/fetch.rs": "service client: research",
    "apps/research/src/media/mod.rs": "service client: research",
    "apps/scientific/src/clients/embeddings.rs": "service client: scientific",
    "apps/scientific/src/clients/inference.rs": "service client: scientific",
    "apps/scientific/src/clients/lean4.rs": "service client: scientific",
    "apps/scientific/src/router/health.rs": "service client: scientific",
    "apps/scientific/src/state.rs": "service client: scientific",
    "apps/server/src/benchmark_producer.rs": "service client: server benchmark",
}


def tracked_sources(root: Path) -> list[str]:
    """Every tracked Rust source file under crates/*/src or apps/*/src."""
    completed = subprocess.run(
        ["git", "-C", str(root), "ls-files", "crates", "apps"],
        capture_output=True, text=True, check=True,
    )
    paths = []
    for line in completed.stdout.splitlines():
        parts = line.split("/")
        if len(parts) < 4 or parts[2] != "src" or not line.endswith(".rs"):
            continue
        if parts[0] not in {"crates", "apps"}:
            continue
        paths.append(line)
    return sorted(paths)


def scan(root: Path) -> tuple[list[str], list[str]]:
    """Return (offenders, stale_entries) for one tree."""
    offenders: list[str] = []
    seen: set[str] = set()
    for path in tracked_sources(root):
        text = (root / path).read_text(encoding="utf-8", errors="replace")
        if MARKER in text:
            seen.add(path)
            if path not in ALLOW:
                offenders.append(path)
    stale = sorted(set(ALLOW) - seen)
    return offenders, stale


def check(root: Path) -> int:
    offenders, stale = scan(root)
    if offenders:
        print("check-egress: refusals — no ad-hoc HTTP outside the allowlist:", file=sys.stderr)
        for path in offenders:
            print(f"  + {path}: references reqwest:: but is not a declared transport", file=sys.stderr)
    if stale:
        print("check-egress: refusals — stale allowlist entries (file no longer uses reqwest::):",
              file=sys.stderr)
        for path in stale:
            print(f"  - {path}", file=sys.stderr)
    if offenders or stale:
        return 1
    print(f"check-egress: OK — {len(ALLOW)} declared transports, "
          f"{len(tracked_sources(root))} source files scanned")
    return 0


def self_test() -> int:
    """A planted offender outside the engine must fail; the allowed file must pass."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "crates" / "actions" / "src").mkdir(parents=True)
        (root / "crates" / "api" / "src").mkdir(parents=True)
        allowed = "crates/actions/src/invoke.rs"
        (root / allowed).write_text("fn e() { let _ = reqwest::Client::new(); }\n", encoding="utf-8")
        planted = "crates/api/src/route.rs"
        (root / planted).write_text("fn r() { let _ = reqwest::Client::new(); }\n", encoding="utf-8")
        subprocess.run(["git", "-C", str(root), "init", "-q"], check=True)
        subprocess.run(["git", "-C", str(root), "add", allowed, planted], check=True)
        broken_allow = {allowed: "engine", "crates/actions/src/gone.rs": "stale entry"}
        saved = dict(ALLOW)
        try:
            ALLOW.clear()
            ALLOW.update(broken_allow)
            offenders, stale = scan(root)
            assert planted in offenders, offenders
            assert allowed not in offenders, offenders
            assert stale == ["crates/actions/src/gone.rs"], stale
            print("check-egress: self-test OK — a planted offender outside the engine is refused")
        finally:
            ALLOW.clear()
            ALLOW.update(saved)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    return check(ROOT)


if __name__ == "__main__":
    raise SystemExit(main())
