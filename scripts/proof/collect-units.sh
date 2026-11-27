#!/usr/bin/env bash
# The unit-ledger collector's entry point (conveyor#836); see collect_units.py.
#
#   collect-units.sh fetch   --out docs/evidence/units/<date>
#   collect-units.sh render  --snapshot docs/evidence/units/<date> --out docs/evidence/units/<date>
#   collect-units.sh certify --run docs/evidence/units/<date> [--token-file ~/.conveyor/<tenant>.token]
#   collect-units.sh report  --run docs/evidence/units/<date>
#   collect-units.sh --self-test
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec python3 -I "$ROOT/scripts/proof/collect_units.py" "$@"
