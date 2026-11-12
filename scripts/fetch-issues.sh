#!/usr/bin/env bash
# Fetch the GitHub issue registry the design site publishes.
#
# The conveyor never calls GitHub at runtime. This script is run before a snapshot/publish and
# its output is committed beside the snapshot; `conveyor snapshot` reads it and never touches
# the network.
#
# ⛔ PUBLIC-SAFE BY CONSTRUCTION. The registry is published on a public site, so it carries the
# number, a redacted title, the state, the labels and the URL only. Issue BODIES and AUTHOR
# LOGINS are never written; titles pass the same redaction as the snapshot (home paths, emails,
# credential-shaped strings, IPs).
#
# Usage: scripts/fetch-issues.sh [out]
#   out defaults to docs/design/data/issues.json
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${1:-$root/docs/design/data/issues.json}"
mkdir -p "$(dirname "$out")"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
gh issue list \
  --state all \
  --limit 500 \
  --json number,title,state,labels,url,updatedAt \
  > "$tmp"

python3 - "$tmp" "$out" <<'PY'
import json
import re
import sys

src, dst = sys.argv[1], sys.argv[2]
HOME_PATH = re.compile(r"(?:/Users/[A-Za-z0-9._-]+|/home/[A-Za-z0-9._-]+|~/)[^\s\"']*")
EMAIL = re.compile(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}")
TOKEN = re.compile(
    r"(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9]{20,}"
    r"|AKIA[0-9A-Z]{16}|xox[baprs]-[A-Za-z0-9\-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----)"
)
IP = re.compile(r"\b(?:\d{1,3}\.){3}\d{1,3}\b")
IPV6ish = re.compile(r"\b(?:[0-9a-fA-F]{1,4}:){2,}[0-9a-fA-F]{0,4}\b")


def clean(text):
    text = HOME_PATH.sub("[redacted-path]", text or "")
    text = EMAIL.sub("***@redacted", text)
    text = TOKEN.sub("[redacted-token]", text)
    text = IP.sub("[redacted-ip]", text)
    return IPV6ish.sub("[redacted-ip]", text)


rows = []
for issue in json.load(open(src, encoding="utf-8")):
    rows.append(
        {
            "number": issue["number"],
            "title": clean(issue["title"]),
            "state": issue["state"],
            "url": issue["url"],
            "updatedAt": issue.get("updatedAt"),
            "labels": [
                {"name": label["name"], "color": label.get("color", "")}
                for label in issue.get("labels", [])
            ],
        }
    )

with open(dst, "w", encoding="utf-8") as handle:
    json.dump({"issues": rows}, handle, indent=2)
    handle.write("\n")
PY

echo "fetch-issues: wrote $out ($(jq '.issues | length' "$out") issues, no bodies or authors)"
