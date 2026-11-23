#!/usr/bin/env bash
# build-docs.py owns only the pages it generates. `docs/design/docs` is also the destination of
# the publish-time platform outputs (`standards/`, `output-*.html`); a rebuild must never delete
# them. This regresses the rebuild that `shutil.rmtree(OUT)` wiped 506 committed output pages.
#
# The test drives the real script in a fixture git repo with a stub pandoc, so it needs no
# pandoc install and no network. It proves:
#   1. an unmanaged sentinel survives a successful rebuild while a managed page is regenerated;
#   2. a failed rebuild leaves both the unmanaged sentinel and the managed pages untouched.
#
# Run: bash scripts/ci/tests/build_docs_preserve.sh
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

root="$tmp/fixture"
stub="$tmp/stub"
mkdir -p "$root/docs/design/scripts" "$root/docs/design/docs/standards" "$root/docs" "$stub"
cp "$repo/docs/design/scripts/build-docs.py" "$root/docs/design/scripts/build-docs.py"

cat > "$root/docs/design/scripts/doc-template.html" <<'HTML'
<!doctype html>
<html><head><title>$title$</title></head>
<body><base href="$root$"><a href="$source_url$">$source$</a><main>$body$</main></body></html>
HTML

printf '# Fixture Note\n\nA managed page.\n' > "$root/docs/note.md"
printf '# Fixture Readme\n' > "$root/README.md"

# Two shapes of publish-time platform output, committed beside the generated pages.
printf 'standards platform output\n' > "$root/docs/design/docs/standards/std-fixture.html"
printf 'published platform output\n' > "$root/docs/design/docs/output-fixture.html"
# A stale managed page: the rebuild must replace it, not merely leave it.
printf 'STALE\n' > "$root/docs/design/docs/note.html"
# An unmanaged file with no extension, like the published `outputs.js`.
printf 'window.CONVEYOR_OUTPUTS = {"sentinel": true};\n' > "$root/docs/design/docs/platform-sentinel.keep"

cat > "$stub/pandoc" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [ "${STUB_PANDOC_FAIL:-0}" = "1" ]; then
  echo "stub pandoc: forced failure" >&2
  exit 1
fi
source="${!#}"
printf '<h1>stub render of %s</h1>\n' "$(basename "$source")"
SH
chmod +x "$stub/pandoc"

git -C "$root" init -q
git -C "$root" add -A
git -C "$root" -c user.name=fixture -c user.email=fixture@example.invalid \
  commit -qm "fixture: docs source and platform outputs"

build="$root/docs/design/scripts/build-docs.py"
run() {
  ( cd "$root" && PATH="$stub:$PATH" STUB_PANDOC_FAIL="${STUB_PANDOC_FAIL:-0}" python3 "$build" "$@" )
}

fail=0
ok() { echo "ok: $1"; }
bad() { echo "FAIL: $1"; fail=1; }
content() { cat "$root/docs/design/docs/$1"; }
preserved() { # name, path
  if [ "$(cat "$root/docs/design/docs/$2")" = "$3" ]; then ok "$1 survives the rebuild"; else bad "$1 was deleted or rewritten"; fi
}

sentinel_standards="standards platform output"
sentinel_output="published platform output"
sentinel_keep='window.CONVEYOR_OUTPUTS = {"sentinel": true};'

# 1. A successful rebuild: managed pages regenerate, platform outputs survive.
if run > "$tmp/build.log" 2>&1; then ok "rebuild exits 0"; else bad "rebuild exited non-zero"; cat "$tmp/build.log"; fi
if [ -f "$root/docs/design/docs/README.html" ]; then ok "missing managed page is generated"; else bad "README.html was not generated"; fi
case "$(content note.html)" in
  *"stub render of note.md"*) ok "stale managed page is regenerated" ;;
  *) bad "note.html was not regenerated"; cat "$root/docs/design/docs/note.html" ;;
esac
preserved "standards/ platform output" "standards/std-fixture.html" "$sentinel_standards"
preserved "output-*.html platform output" "output-fixture.html" "$sentinel_output"
preserved "extension-less platform output" "platform-sentinel.keep" "$sentinel_keep"

# 2. --check still verifies the freshly generated pack, and --verify-render still renders twice.
if run --check > "$tmp/check.log" 2>&1; then ok "--check passes after the rebuild"; else bad "--check failed after the rebuild"; cat "$tmp/check.log"; fi
if run --verify-render > "$tmp/verify.log" 2>&1; then ok "--verify-render passes after the rebuild"; else bad "--verify-render failed after the rebuild"; cat "$tmp/verify.log"; fi
preserved "standards/ platform output after --verify-render" "standards/std-fixture.html" "$sentinel_standards"

# 3. A failed rebuild installs nothing: no managed page and no platform output is touched.
printf 'STALE\n' > "$root/docs/design/docs/note.html"
rc=0
STUB_PANDOC_FAIL=1 run > "$tmp/fail.log" 2>&1 || rc=$?
if [ "$rc" -ne 0 ]; then ok "failed rebuild exits non-zero"; else bad "failed rebuild exited 0"; fi
case "$(content note.html)" in
  STALE*) ok "failed rebuild leaves the managed page untouched" ;;
  *) bad "failed rebuild overwrote note.html"; cat "$root/docs/design/docs/note.html" ;;
esac
preserved "standards/ platform output" "standards/std-fixture.html" "$sentinel_standards"
preserved "output-*.html platform output" "output-fixture.html" "$sentinel_output"
preserved "extension-less platform output" "platform-sentinel.keep" "$sentinel_keep"

if [ "$fail" -ne 0 ]; then
  echo "build_docs_preserve: FAILURES" >&2
  exit 1
fi
echo "build_docs_preserve: all checks passed"
