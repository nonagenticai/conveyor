# T14 live gate — measured 2026-11-25, executed 2026-11-26

The gate (`acceptance/programme/T14.toml`, SPEC_V2 §12 T14) requires, for each published
toolkit, a probe and one live call; a `scientific_processor` scale-up/scale-back receipt; a
zero-use report; and a backend outage that unbinds new runs while in-flight runs finish.

## 2026-11-26 execution — all four clauses

Destinations were reached from m4 over ssh port-forwards (`:22` is the only port the tailnet
admits): mini2 tts `:9202` / asr `:9201`, mini1 image `:9204`, MinIO on mf1 `:8389`, and the
in-cluster `scientific-processor` `:8808`, `coding` `:8687`, `strata-rag` `:8092` and `licences`
`:8544` through `kubectl port-forward` on strato. The branch binary ran locally against the
replay record (tenant `nonagentic`); every call below is a real HTTP call to the estate service
and leaves a `tool_call` row.

| Clause | Result | Receipts |
|---|---|---|
| probe per published toolkit | **PASS** — coding, speech, transcription, image_gen, retrieval, scientific all `live` (200) | `2026-11-26/01-probe.json` |
| one live call per published toolkit | **PASS** — six calls, each a destination answer (below) | `2026-11-26/14-call-*.json` |
| `scientific_processor` scaled up for its probe and call, then back | **PASS** — the drill recorded 2026-11-25 against the same deployment; after restore the probe is live and the call reached the service | `23-scale-zero.log`, `24-scale-up.log`, `26-scale-outage.log`, `29-scale-restore.log`, `31-scale-outage-2.log`, `34-scale-restore-2.log`, `36-probe-restored-final.json` |
| a zero-use report exists | **PASS** — `tools.usage`, 30-day window, one row per published tool version | `2026-11-26/02b-zero-use-after.json` (pre-call: `02-zero-use.json`) |
| an outage unbinds new runs and in-flight runs finish | **PASS** — with the scientific pods gone the probe reported `unreachable`, two bindings were disabled, and `tools.call` was refused `409` before any network call; the in-flight half is the Rust test | `32-probe-outage-2.json`, `33-call-outage-2.json`, `crates/actions/tests/estate_toolkits.rs::an_in_flight_run_finishes_after_the_unbind` |

### The six live calls

| Toolkit | Tool | Run | Answer |
|---|---|---|---|
| speech | `speech.synthesize` | `01a21a03-69b2-71d3-930f-ac6d5bec70d7` | 320 KB WAV, document `01a21a03-711d-727f-a796-7c42d3543f7a` |
| transcription | `media.transcribe` | `01a21a03-7d75-7ad3-94de-ec38ea8ad4ca` | the transcript text, from the speech document |
| coding | `coding.jobs_create` | `01a21a03-f612-75c9-9fa2-12a6b0c2b95b` | job `7104f3e6-f814-4f91-a1e0-8a1c6bc30285` queued |
| image_gen | `image_gen.generate` | `01a21a04-03b3-7f69-9a7c-f570fb0fe8e5` | 512×512 PNG, document `01a21a05-1955-711a-897b-8d67ce3fb741` |
| scientific | `scientific_stats.meta_analysis` | `01a21a05-2417-7472-9e2a-aaf4c3a88770` | pooled effect 0.3 |
| retrieval | `retrieval.query` | `01a21a06-aea7-7f8a-9d81-c4b02971fa4b` | a real strata_rag answer (routing hybrid; zero hits — this tenant has no indexed leaves) |

Each binding and its effect ceiling is in `2026-11-26/13-bind-<toolkit>.json`; each request body is
in the matching `-input.json`. The publication wave is `00-publish.json`: six published, two
draft (webscraper, lean4).

### What had to be true to get there — measured, not assumed

- **Reachability.** All four hosts were up the whole time; the 2026-11-25 note "unreachable" was
  the ssh invocation, not an outage. `id_ed25519_mini` and `id_ed25519_mini2` are
  passphrase-protected and `-o UseKeychain=yes` is what unseals them. The tailnet ACL admits only
  `:22` from m4, so every service was reached through a tunnel.
- **Spend budget.** A `spend`-class tool is refused `429` unless the run declares
  `limits.cost_budget_usd`. A plain run has no job binding, so the run's inputs are written by the
  operator (the agent runtime is the production writer); the write and the read-back are in
  `2026-11-26/11b-budget-*.json`.
- **Coding contract drift.** The coding service requires an `Idempotency-Key` on the call (the
  broker claims it) and its current create body is
  `input.source = {kind:"inline", files:[{path,text}]}`; the older `files` map is a `422`.
- **Retrieval's schema.** `retrieval.query` had an empty input schema, so the published tool could
  only send `{}`; it now declares a closed `{query, top_k}` body.
- **Credentials.** The tenant's sealed `licence.token` is admitted by the TTS gate (its
  `W2V_LICENSE_GATE_TOKEN_APP_KEYS` is `speech`), by ASR (its entitlement gate is dormant, so any
  active licence is admitted) and by scientific (introspection only). `strata_rag` accepts only app
  `strata-rag`. The licences bundle-token mint was refused `403 Role required:
  bundle-token-minter` — a Keycloak realm role the conveyor's secure-apps client does not hold
  (the #776 gap). The owner chose the strata-rag service token instead: the `*:<token>` entry of
  `STRATA_API_TOKENS`, sealed as `strata.token` and never printed; the binding sends it as the
  Authorization bearer with the bare `X-Tenant-Id`. The refusal is kept at
  `2026-11-26/03-mint-strata.err`.
- **Object storage.** Speech, transcription and image_gen store documents; the local engine used
  the deployment's MinIO bucket (`conveyor-documents`) through an mf1 tunnel, with the S3 keys
  read from the deployment's SOPS secret.

## 2026-11-25 measurement (history)

The first run of the gate found the code defects below and classified four calls as blocked:
speech/transcription `503` (the staging copy carried no sealed `licence.token`), scientific `401`
(the action sent no licence header), image_gen `503` (no object storage in the local engine).
Those classes were real; the estate was not down. The 2026-11-26 execution above resolves them.

## Product defects the live gate found

1. **The outbound Idempotency-Key was never forwarded** (2026-11-25). The coding service refuses a
   create without it; the engine claimed the key for its own dedupe and sent nothing. The external
   dispatcher now forwards the caller's key, and the admin CLI can carry one.
2. **`scientific` was not publishable** (2026-11-25). Its manifest status was `draft` while the
   gate named its probe; it is published.
3. **`retrieval.query` could not carry a query** (2026-11-26). The action's input was empty while
   strata_rag requires `query`; the schema now declares `{query, top_k}`.
4. **The scientific and retrieval actions sent no credential** (2026-11-26). Their toolkits now
   declare the `sealed-profile` adapter and their bindings carry the tenant secret, so the
   dispatcher sends `Authorization: Bearer <secret>` with the bare `X-Tenant-Id`.

## Open, not gate-blocking

- Retrieval returns zero hits for this tenant: it has no indexed leaves. The call reached the
  service and was ledgered; an empty corpus is not a failed call.
- Issue 561 (research and Lean toolkits, first wave) remains open; `webscraper` and `lean4` are
  draft and outside the published-toolkit clause.
- T14 stays `active` by its acceptance manifest; closure is not part of this gate.
