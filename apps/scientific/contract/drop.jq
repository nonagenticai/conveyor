# The surfaces this rewrite deliberately does not carry.
#
# One decision, applied one way: an operation whose only data source is gone, or whose runtime
# already exists as another conveyor tool, is not ported. A client to a dead service is not a
# feature, and a second code-execution runtime is not either.
#
# The decision is recorded in docs/decisions/0006-tool-absorption-grouping.md. If any of these is
# ever wanted back it is a new capability with a live upstream — never a restoration of this
# capture, and never a fixture that fakes the missing store.
#
# Graph adapter / discovery engine — the store was destroyed on 2026-11-01, both backends are gone,
# and no named consumer exists. (Same reason as the topology and explain surfaces.)
#
#   /api/verify/cove          gathers evidence through the discovery engine and the graph
#   /api/verify/integrity     the graph adapter is its only input
#   /api/pipeline/trigger     reads the entity and its neighbours from the graph
#   /api/pipeline/webhook     the same read, behind a background task
#   /api/pipeline/batch       asks the graph which entities are unscored
#
# Kubernetes Jobs — the conveyor already dispatches execution as `coding_jobs.create` /
# `coding_jobs.get` against the coding service, so this would be a second runtime for one job.
#
#   /api/sandbox/execute
#   /api/sandbox/jobs/{job_id}
#   /api/sandbox/results/{job_id}
.paths |= with_entries(
    select(
        (.key | startswith("/api/topology/") or startswith("/api/explain/")) | not
    )
    | select(.key != "/api/verify/cove")
    | select(.key != "/api/verify/integrity")
    | select(.key != "/api/pipeline/trigger")
    | select(.key != "/api/pipeline/webhook")
    | select(.key != "/api/pipeline/batch")
    | select((.key | startswith("/api/sandbox/")) | not)
)
