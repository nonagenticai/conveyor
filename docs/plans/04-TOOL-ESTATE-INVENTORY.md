# 04 — The tool estate: what remains, and the deliverable vocabulary

**Status:** recording document · 2026-11-15 · unit #138
**Authority above this file:** `docs/decisions/0006-tool-absorption-grouping.md` (the grouping),
`docs/decisions/0005-absorption-sequence-and-repo-scope.md` (who performs a rewrite).

This file is not a new plan and opens no new direction. ADR 0006 decided the grouping and the
sequence. What was missing was three facts in one place: where each group actually stands, that the
eighteen stage directories are a **deliverable vocabulary rather than services**, and that the estate
has **one capability gap no spec covers** — host control.

---

## 1. Where each ADR 0006 group stands

A group is one deployable with one image, one database and one role; the toolkits it publishes stay
separate catalogue entries (`§11.2` identities), so the broker never learns the topology.

| Group | Absorbs, by role | State |
|---|---|---|
| `apps/scientific` | the scientific research service; the evidence-review service | **absorbed, not deployed** (#105 merged). The live deployment is still the Python image and sits at zero replicas |
| `apps/research` | the web-scraper service; the media-discovery service | **in flight** (#117 claimed, #118 blocked) |
| `apps/media` | the browser-rendered producers; the ffmpeg producers; the thin speech fan-out and video-publish operations | **decided, unclaimed** (#139) |
| `apps/lean4-prover` | the Lean 4 prover | **absorbed, not deployed** (#116 merged). The live deployment is still the pre-absorption image; the deployable stays separate for its toolchain state and node pin |
| `apps/coding` | the `coding-*` fleet | **join the existing Rust repository, never a second rewrite** (ADR 0006). The Python fleet is still deployed and is not the current implementation |
| external — `strata_ingest`, `strata_rag` | shared ingest and retrieval | done (ADR 0005 §2); they are tool-estate members but not repository scope |
| external — the model servers | speech, transcription, image generation, vision | **stay remote HTTP hosts bound as toolkits** (ADR 0006 §Consequences) |

Two lessons this table makes visible, both recorded rather than fixed here:

- **"Merged" and "live" are different claims.** Two groups are fully absorbed in code and still
  serve the legacy image. Any statement that the estate is Rust must be read as "the rewrite exists",
  not "the old service is off".
- **`apps/media` is the one decided group with no issue.** ADR 0006 §Consequences says "one issue
  per app"; #139 supplies the missing one.

### 1.1 Archived and needing no decision

Five stage blueprints were archived before this document was written. They are not migration
targets and must not be reopened as work.

---

## 2. The eighteen stage directories are a vocabulary, not services

ADR 0006 records the correction: the "w2v stages" are not services — they live inside the
orchestrator, which is out of scope. That correction is load-bearing and easy to lose, so it is
restated here with the content that gives it meaning.

Of the eighteen directories, **two are libraries** (`primitives`, `shared`). The other **sixteen are
deliverable blueprints** — each names a kind of output the system can produce and the sequence of
steps that produces it. They are the conveyor's **project-type vocabulary**.

| Blueprint | Deliverable | Phases |
|---|---|---|
| `generic` | any short media piece | brief → source ingest → script → storyboard → asset search → site capture → stills → generated video → motion graphics → narration → render |
| `lesson` | one narrated lesson | script → slides → stills → narration → record → compile |
| `course` | a multi-lesson course | outline → per-lesson compilation → stitch → thumbnail |
| `audiobook` | a narrated long-form reading | script → narration → compile |
| `promo` | a promotional clip | brief → script → assets → render |
| `demo_video` | a product demonstration | brief → screen capture → narration → render |
| `action_reel` | a montage | brief → assets → render |
| `app_review` | an application assessment | capture → inspect → analyse → report |
| `audit_video` | a video quality audit | extract frames → OCR → vision verification → report |
| `website_review` | a site assessment | capture → analyse → report |
| `web_design` | a redesigned site | capture → redesign → screenshot → deliver |
| `web_design_v2` | the redesign, second generation | as above with the current planner |
| `seo_audit` | a search audit | crawl → analyse → report |
| `cro_audit` | a conversion audit | crawl → analyse → report |
| `marketing_scorecard` | a scored campaign read | gather → score → report |
| `growth_bundle` | a combined growth pack | the audit set, composed |

**Consequence — the point of this section.** These sixteen map to **project types and procedure
templates**, never to services. A project type is a template, not code (`SPEC_V2` §5.2): its phases
are WBS node kinds, its gates are approval rows, its artefacts are governed assets. So "produce a
video course" is a **project** with a baseline, gates and evidence — not a DAG a person re-invokes,
and not a service to port.

They divide on exactly the line the owner drew:

| Family | Blueprints | What the output is for |
|---|---|---|
| **analysis** | `app_review`, `audit_video`, `website_review`, `web_design`, `web_design_v2`, `seo_audit`, `cro_audit`, `marketing_scorecard`, `growth_bundle` | **evidence** — a gate or QC input |
| **production** | `generic`, `lesson`, `course`, `audiobook`, `promo`, `demo_video`, `action_reel` | **an artefact** — a published deliverable |

---

## 3. The two capability families

Declared in `SPEC_V2` §10.4. One note on the boundary, because it is the defect a later reader
would otherwise introduce:

- `analyse.*` concerns **artefacts and external properties** — a site, an application, a video, brand
  presence. Its outputs are evidence.
- `metrics.*` and the eval slices (S17–S20) concern **this system's own process quality** — flow,
  quality, cost, governance, and scored evaluations.

A capability that scores *our* work is `metrics`/evals. A capability that scores *the thing we were
asked to look at* is `analyse.*`. Building the second inside the first is the double-build this note
exists to prevent.

`produce.*` is the production side of the same table, and it is deliberately one family: the sixteen
blueprints differ in ordering and in which capabilities they call, not in kind.

---

## 4. The gap: host control

`SPEC_V2` §10.4 declares the operations family with `ops.k8s_*` and `ops.git_*`. There is **no
family that reaches a bare-metal host** — no service restart, no unit status, no log tail on a
machine that is not a cluster node. Several estate hosts are exactly that, and the model servers
bound as toolkits live on them.

The reason this is a design gap and not a missing implementation detail: the conveyor runs in the
cluster. Reaching a host means a credential that can restart anything on it, held by a web-facing
service. That is a different **class** of credential from a namespaced service account, and it
deserves its own decision rather than appearing as a function of `ops.k8s_restart`.

The shape proposed for consideration (§10.4 declares the family; the mechanism is T7's to settle):

1. A **host registry** — a row naming an addressable host and its role, so a job never supplies one.
2. A **per-host agent** on each host, and no host credential inside the conveyor pod. The agent is
   the credential boundary, mirroring the rule the broker already applies to tool destinations.
3. A **forced-command** key: the agent answers a fixed verb set, so a compromised conveyor cannot
   ask for an arbitrary command.
4. The **destructive effect class** already applies — approval, dry run, dual control.

Until that is decided, host control does not exist, and a procedure must not pretend to do it.

---

## 5. Destination count

Measured by role, without naming hosts or repositories:

- **~30 tool-adjacent services are deployed today** across the legacy systems and the estate.
- The grouping above reduces that to **~15 destinations**, of which **7 are this workspace's own
  applications** (`server`, `web`, `inference-router`, `licenses`, `scientific`, `lean4-prover`,
  `coding`), and the rest are bound toolkits: the two shared Rust stores, the model servers, the
  research group, and the media group.

The reduction is not the goal. ADR 0006 §2 is: **combine on runtime shape, not on file count.** The
browser and the ffmpeg producers do not share a runtime, so they do not become one image; the thin
operations share the server's runtime, so they do not become a deployable at all.

---

## 6. What stays outside, deliberately

- **The model servers.** They are model servers, not tool code (ADR 0006 §Consequences).
- **Product applications** are clients of this system, never job tools (`§11.2`).
- **The agent platform and its gateway.** Two control planes, permanently (D59).
- **The orchestrator.** Out of scope; its role becomes procedures, not a second scheduler.

---

## 7. Open items this document raises

1. `apps/media`'s device-inspection member needs an Appium driver and a device farm, which no other
   member of the group does. Absorb, or keep as a bound external tool? Recorded on #139.
2. Two fleet functions are claimed by neither the Rust service nor a measured consumer. Neither
   port nor drop is justified until the consumer is measured (ADR 0006 §row `apps/coding`).
3. Two groups are merged but not deployed. A deployment unit each — not part of this document.
4. Host control is a family without a mechanism. §4 above is a proposal for T7, not a decision.
