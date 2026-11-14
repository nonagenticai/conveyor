# The two surfaces this rewrite deliberately does not carry.
#
# Both are backed by the graph adapter, whose store was destroyed on 2026-11-01, and neither has a
# named consumer. The drop is a decision (docs/decisions/0006-tool-absorption-grouping.md), not an
# omission: if either surface is ever wanted back, it is a new capability with a live upstream,
# never a restoration of this capture.
.paths |= with_entries(
    select(.key | startswith("/api/topology/") or startswith("/api/explain/") | not)
)
