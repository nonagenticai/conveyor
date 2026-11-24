# T19 close-out — `programmes.refresh` evidence

Intent: after `migrate --seed --tenant nonagentic` installed the buildout (catalogue, programme,
benefits, tranches), this call proves the action path is idempotent: it must create no row, update
none, and report the seeded programme as skipped. The read-back is `programmes.list`; the write and
its read-back are recorded beside this file in `refresh-readback.json`.

Measured 2026-11-24 on the deployed engine (revision `a8f67c3e`).
