#!/usr/bin/env bash
# Regenerate the pre-rendered Mermaid SVG pairs used by the System architecture page.
# The generated files are committed; running this script is only needed after editing a .mmd.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v mmdc >/dev/null 2>&1 || {
  echo "mmdc (mermaid-cli) is required to regenerate diagrams; the committed SVGs are the runtime assets." >&2
  exit 1
}
if [ -z "${PUPPETEER_EXECUTABLE_PATH:-}" ]; then
  for candidate in "${CHROME_BIN:-}" "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"; do
    if [ -n "$candidate" ] && [ -x "$candidate" ]; then
      export PUPPETEER_EXECUTABLE_PATH="$candidate"
      break
    fi
  done
fi
for src in architecture/diagrams/*.mmd; do
  base="${src%.mmd}"
  mmdc -i "$src" -o "${base}.light.svg" -c architecture/mermaid-light.json -b transparent -w 1600 -q
  mmdc -i "$src" -o "${base}.dark.svg" -c architecture/mermaid-dark.json -b transparent -w 1600 -q
done
echo "rendered $(ls architecture/diagrams/*.light.svg | wc -l | tr -d ' ') diagram(s) in light and dark"
