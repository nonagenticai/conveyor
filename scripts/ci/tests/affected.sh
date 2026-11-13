#!/usr/bin/env bash
# Fixture matrix for scripts/ci/affected.sh. No compilation, no network.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

A="bash scripts/ci/affected.sh"
fail=0

check_has() { # name, haystack, needle
  case " $2 " in
    *" $3 "*) echo "ok: $1" ;;
    *) echo "FAIL: $1 (missing '$3' in: $2)"; fail=1 ;;
  esac
}

check_lacks() { # name, haystack, needle
  case " $2 " in
    *" $3 "*) echo "FAIL: $1 (unexpected '$3' in: $2)"; fail=1 ;;
    *) echo "ok: $1" ;;
  esac
}

check_eq() { # name, got, want
  if [ "$2" = "$3" ]; then
    echo "ok: $1"
  else
    echo "FAIL: $1 (got '$2', want '$3')"
    fail=1
  fi
}

total=$($A --files "Cargo.lock" --json | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')

out=$($A --files "apps/server/src/lib.rs")
check_has "apps/server selects conveyor-server" "$out" "conveyor-server"

out=$($A --files "crates/domain/src/lib.rs")
check_has "crates/domain selects conveyor-domain" "$out" "conveyor-domain"
check_has "crates/domain selects conveyor-store" "$out" "conveyor-store"
check_has "crates/domain selects conveyor-server" "$out" "conveyor-server"

out=$($A --files "crates/platform/src/lib.rs")
check_has "crates/platform selects itself" "$out" "conveyor-platform"

out=$($A --files "")
check_eq "an empty diff selects nothing" "$out" ""

out=$($A --files "Cargo.lock")
n=$($A --files "Cargo.lock" --json | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')
check_eq "Cargo.lock selects every package" "$n" "$total"

out=$($A --files "scripts/ci/gate.sh")
n=$($A --files "scripts/ci/gate.sh" --json | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')
check_eq "scripts/ci selects every package" "$n" "$total"

out=$($A --files "README.md")
check_eq "an unrelated file selects nothing" "$out" ""

out=$($A --files "Cargo.lock" --apps)
check_has "--apps keeps apps packages" "$out" "conveyor-server"
check_lacks "--apps drops crates packages" "$out" "conveyor-domain"

exit "$fail"
