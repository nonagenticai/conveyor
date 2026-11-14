# licenses — captured contract

`openapi.lock.json` is the OpenAPI 3.1 document the live service served when the absorption
started (43 paths, 60 operations, 57 schemas). `manifest.json` records when it was captured, the
deployed image digest and the lock's own sha256.

The lock is the parity baseline:

- the Rust service serves the same document and proves it with `--check-lock`;
- the golden replay and the shadow diff use this document's paths and schemas.

Re-capture only when the live service changes on purpose, and explain the diff in the same commit:

```sh
./capture.sh --live https://<licenses-host> \
    --image-digest sha256:<digest> \
    --compare <old-service>/docs/openapi.json
```

`capture.sh` needs `curl`, `jq` and `shasum` (or `sha256sum`). The service address is supplied at
capture time and is never stored in this tree. `--compare` writes `compare-report.txt`: the
structural difference between the live lock and the old repository's committed snapshot.

The 2026-11-14 capture was **byte-identical after `jq -S`** to `docs/openapi.json` at
`licenses@4e8a08d25a239c5bd5c563e8fc5f49d6576e71e4` — the commit the live image's `sha-4e8a08d`
tag names.

## The schema — `schema.lock.sql`, `schema-diff.sh`

`schema.lock.sql` is `pg_dump --schema-only --no-owner --no-privileges --no-tablespaces` of the
live `licenses` database (22 tables; 24 relations including the two `pg_stat_statements` views).
It is the live database's truth, not `models.py`'s: the Python service builds a fresh database from
SQLAlchemy's `create_all` and its migration runner stamps pending files without replaying them, so
a fresh Python database and production genuinely diverge.

`schema-diff.sh` starts a throwaway `postgres:18.6`, applies `../migrations/*.sql` in order, dumps
it with the same options and diffs the two. `CHECK` expressions are compared as token streams: the
same `IN (...)` predicate deparses with different, semantically irrelevant parenthesisation and
redundant casts depending on how it was parsed. Every other line is compared verbatim.

Production can be re-dumped with:

```sh
sudo -u postgres pg_dump --schema-only --no-owner --no-privileges --no-tablespaces -d licenses
```

## Still to come (the parity slice)

`capture-goldens.sh`, `oracle.sh`, `normalize.sh`, `tolerances.json` and `shadow.sh` follow the
`apps/inference-router/contract/` shape. They need the pinned old image booted with a synthetic
seed; the golden set must never carry a production tenant, app key or token. Until they land,
`--check-lock` is the only gate in this directory.
