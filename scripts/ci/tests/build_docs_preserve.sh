#!/usr/bin/env bash
# The document renderer must not delete what the live publish installed.
#
# `build-docs.py` replaces `docs/design/docs` wholesale. The live publish (`publish-data.sh` §3e)
# and the pack publish (`publish-standards.sh`) install `outputs.js`, `output-*.html` and
# `standards/` there. Measured twice: #515/#516 restored the T16 pages by hand, and #723 left the
# null placeholder over the published index — which moved the conformance audit's gap register and
# redded main's design-pack (run 45577495051).
#
# Proven here, without pandoc (a stub renders the fragment):
#   1. a published index, page and standards tree survive a regeneration byte-identically, while
#      the document pages and index are still written;
#   2. a clean checkout with no index still gets the null placeholder, and check-outputs parses it.
#
# Run: bash scripts/ci/tests/build_docs_preserve.sh
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

plant() { # $1 = root
  local root="$1"
  mkdir -p "$root/docs/design/scripts" "$root/docs/design/docs" "$root/docs/plans" "$root/bin"
  cp "$repo/docs/design/scripts/build-docs.py" "$root/docs/design/scripts/build-docs.py"
  cp "$repo/docs/design/scripts/doc-template.html" "$root/docs/design/scripts/doc-template.html"
  printf '# Fixture plan\n\nA source document.\n' > "$root/docs/plans/00-INDEX.md"
  printf '# Fixture document\n\nAnother source.\n' > "$root/docs/sample.md"
  cat > "$root/bin/pandoc" <<'SH'
#!/usr/bin/env bash
# The write path needs a renderer; this test is about preservation, so echo the source fragment.
for last; do :; done
cat "$last"
SH
  chmod +x "$root/bin/pandoc"
  git -C "$root" init -q
  git -C "$root" -c user.email=t@example.invalid -c user.name=t add -A
  git -C "$root" -c user.email=t@example.invalid -c user.name=t commit -qm fixture
}

run_build() { (cd "$1" && PATH="$1/bin:$PATH" python3 docs/design/scripts/build-docs.py) ; }

# --- 1. the publish's entries survive a regeneration -----------------------------------------
mkdir -p "$tmp/one/docs/design/docs/standards"
plant "$tmp/one"
printf 'window.CONVEYOR_OUTPUTS = {"schema":"design-outputs/v1","outputs":[{"path":"docs/output-01.html"}]};\n' \
  > "$tmp/one/docs/design/docs/outputs.js"
printf '<html>published output page</html>\n' > "$tmp/one/docs/design/docs/output-01.html"
printf '<html>standards index</html>\n' > "$tmp/one/docs/design/docs/standards/index.html"
cp "$tmp/one/docs/design/docs/outputs.js" "$tmp/expected-outputs.js"
cp "$tmp/one/docs/design/docs/output-01.html" "$tmp/expected-output-01.html"
cp "$tmp/one/docs/design/docs/standards/index.html" "$tmp/expected-standards-index.html"
git -C "$tmp/one" -c user.email=t@example.invalid -c user.name=t add -A
git -C "$tmp/one" -c user.email=t@example.invalid -c user.name=t commit -qm publish
run_build "$tmp/one" >/dev/null
cmp "$tmp/expected-outputs.js" "$tmp/one/docs/design/docs/outputs.js"
cmp "$tmp/expected-output-01.html" "$tmp/one/docs/design/docs/output-01.html"
cmp "$tmp/expected-standards-index.html" "$tmp/one/docs/design/docs/standards/index.html"
test -f "$tmp/one/docs/design/docs/plans/00-INDEX.html"
test -f "$tmp/one/docs/design/docs/documents.json"
echo "build_docs_preserve: the published index, page and standards tree survived; documents rendered"

# --- 2. a clean checkout still gets the parseable placeholder --------------------------------
plant "$tmp/two"
run_build "$tmp/two" >/dev/null
grep -qF 'window.CONVEYOR_OUTPUTS = null;' "$tmp/two/docs/design/docs/outputs.js"
cp "$repo/docs/design/scripts/check-outputs.cjs" "$tmp/two/docs/design/scripts/check-outputs.cjs"
(cd "$tmp/two" && node docs/design/scripts/check-outputs.cjs)
echo "build_docs_preserve: a clean checkout gets the null placeholder and check-outputs parses it"
