//! `migrate --dev-seed`: the two development tenants, one user each, their memberships and
//! active dev licences.
//!
//! CONTRACT §10 and S02 §6 step 9: `dev-a`/`alice` and `dev-b`/`bob`, so `Bearer dev:alice`
//! IS the dev-a principal and every later gate that reads
//! `select id from tenant where slug='dev-a'` finds it.
//!
//! This lives in `store` because `store` is the only crate that speaks SQL (CONTRACT §1).
//! `apps/server` keeps a thin `seed` facade so the binary and its tests spell the call the
//! same way they always did.
//!
//! Identity is stored in `app_user.oidc_subject` as the literal `dev:<login>` — there is no
//! login column, and the partial unique index is on `oidc_subject`.
//!
//! Idempotent, precisely: tenants, users and memberships are inserted with `DO NOTHING`, so a
//! second run leaves them byte-identical. Two deliberate exceptions. The licence row is
//! upserted to `status='active'` with a fresh `checked_at` and a bumped `version`, because a
//! stale entitlement would make the dev principals' authenticated invocations a 403. The S16
//! measurement fixture (twelve `inference_call` rows for `dev-a`) keeps its fixed ids but
//! refreshes `created_at`, so the 7-day usage window still contains it on a later day. A
//! second run changes no other row.

use std::sync::OnceLock;

use conveyor_domain::{IdError, TenantId};
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::capability::{
    CapabilityImport, CapabilityStoreError, RelationImport, import_capabilities_tx,
    import_relations_tx,
};
use crate::catalogue::{
    CardTypeImport, CatalogueStoreError, ImportCounts, ImportEntry, import_card_types_tx,
    import_entries_tx,
};
use crate::parent::{Parent, ParentError, ensure_parent_tx};
use crate::scope::begin_scoped;
use crate::tenants;

/// The tenants, as `(slug, display name)`.
///
/// `kata` is the improvement tenant and `execution` the real-project-run tenant (George
/// 2026-11-09); both belong to the seeded platform admin `fr-test`, so a fresh dev database can
/// exercise the Platform screen without provisioning.
pub const DEV_TENANTS: [(&str, &str); 4] = [
    ("dev-a", "Dev A"),
    ("dev-b", "Dev B"),
    ("kata", "Kata"),
    ("execution", "Execution"),
];

/// `(oidc_subject, email, display_name, tenant slug)` for the one member of each tenant.
pub const DEV_USERS: [(&str, &str, &str, &str); 2] = [
    ("dev:alice", "alice@dev.invalid", "Alice", "dev-a"),
    ("dev:bob", "bob@dev.invalid", "Bob", "dev-b"),
];

/// The seeded platform admin: `fr-test`, admin of `kata` and `execution`, holder of
/// `platform_admin`. One dev principal stands in for the owner's Keycloak test login.
pub const DEV_PLATFORM_ADMIN: (&str, &str, &str) =
    ("dev:fr-test", "fr-test@dev.invalid", "fr-test");

/// The tenants the platform admin owns.
pub const DEV_PLATFORM_TENANTS: [&str; 2] = ["kata", "execution"];

/// Failures while seeding.
#[derive(Debug, thiserror::Error)]
pub enum SeedError {
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("seeded row has an unusable id: {0}")]
    Id(#[from] IdError),
    #[error("the seed did not find its own {0} row for {1:?}")]
    Missing(&'static str, &'static str),
    /// A row the seed just inserted did not resolve in its tenant scope.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The embedded catalogue document is not valid JSON (a build-time constant, so this is
    /// a programming error, never a data error).
    #[error("the embedded catalogue seed is not valid JSON: {0}")]
    Catalogue(String),
    /// The embedded capability document is not valid JSON (also a build-time constant).
    #[error("the embedded capability seed is not valid JSON: {0}")]
    CapabilityDocument(String),
    /// The capability importer refused a seed row.
    #[error(transparent)]
    Capability(#[from] CapabilityStoreError),
    /// The eval fixture could not compute its instrument digest or split.
    #[error("eval fixture: {0}")]
    Eval(String),
    /// The embedded benchmark document is not valid JSON (also a build-time constant).
    #[error("the embedded benchmark seed is not valid JSON: {0}")]
    BenchmarkDocument(String),
    /// The embedded programme document is not valid JSON (also a build-time constant).
    #[error("the embedded programme seed is not valid JSON: {0}")]
    ProgrammeDocument(String),
    /// The embedded plan document is not valid JSON (also a build-time constant).
    #[error("the embedded plan seed is not valid JSON: {0}")]
    PlanDocument(String),
    /// The plan seed names a row this tenant does not have.
    #[error("the plan seed names no {0} row {1:?}")]
    PlanMissing(&'static str, String),
    /// The board helper refused a plan row.
    #[error(transparent)]
    Board(#[from] crate::board::BoardStoreError),
    /// The benchmark importer refused the artifact (a seed conflict, a bad row).
    #[error(transparent)]
    Benchmark(#[from] crate::benchmark::BenchmarkStoreError),
    /// The catalogue importer refused a row: an unknown kind, an invalid code, or a
    /// database rejection.
    #[error(transparent)]
    Import(#[from] CatalogueStoreError),
    /// A slug that names no tenant. Naming the slug is the point: `--tenant` is typed by
    /// hand, and a typo must not be mistaken for "every tenant".
    #[error("no tenant with slug {0:?}")]
    UnknownTenant(String),
}

/// Insert the development tenants, users, memberships and licence rows. Refused by the
/// caller unless `APP_ENV` is an explicit development/test value (the seed itself does not
/// read the environment).
pub async fn dev(pool: &PgPool) -> Result<(), SeedError> {
    for (slug, name) in DEV_TENANTS {
        sqlx::query(
            "INSERT INTO tenant (slug, name) VALUES ($1, $2) ON CONFLICT (slug) DO NOTHING",
        )
        .bind(slug)
        .bind(name)
        .execute(pool)
        .await?;
    }

    for (subject, email, display_name, _) in DEV_USERS {
        sqlx::query(
            "INSERT INTO app_user (oidc_subject, email, display_name)
             VALUES ($1, $2, $3)
             ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL DO NOTHING",
        )
        .bind(subject)
        .bind(email)
        .bind(display_name)
        .execute(pool)
        .await?;
    }

    for (subject, _, _, tenant_slug) in DEV_USERS {
        let tenant_id: Uuid = sqlx::query_scalar("SELECT id FROM tenant WHERE slug = $1")
            .bind(tenant_slug)
            .fetch_optional(pool)
            .await?
            .ok_or(SeedError::Missing("tenant", tenant_slug))?;
        let user_id: Uuid = sqlx::query_scalar("SELECT id FROM app_user WHERE oidc_subject = $1")
            .bind(subject)
            .fetch_optional(pool)
            .await?
            .ok_or(SeedError::Missing("app_user", subject))?;

        // membership is FORCE-RLS'd: the insert must run in a tenant-scoped transaction,
        // even as the migration owner (that is exactly what FORCE means).
        let tenant = TenantId::from_uuid(tenant_id)?;
        let mut tx = begin_scoped(pool, tenant).await?;
        // The membership names a global app_user id. The FK alone accepts any existing user,
        // so the seed resolves it in this transaction like every other parent (CONTRACT §2).
        ensure_parent_tx(&mut tx, tenant, Parent::AppUser, user_id).await?;
        sqlx::query(
            "INSERT INTO membership (tenant_id, user_id, role)
             VALUES ($1, $2, 'admin')
             ON CONFLICT (tenant_id, user_id) DO NOTHING",
        )
        .bind(tenant_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
        // S04 decision 9: an invocation is licence-gated, so the dev principals must hold
        // an ACTIVE, freshly-checked entitlement or the gate's authenticated call would be
        // a 403 `licence_required` instead of reaching validation. Status is the
        // vocabulary's `active`; `checked_at = now()` is inside the extractor's staleness
        // window. This is the one row a second seed run rewrites (module doc).
        sqlx::query(
            "INSERT INTO app_licence (tenant_id, app_key, status, checked_at)
             VALUES ($1, $2, 'active', now())
             ON CONFLICT (tenant_id, app_key) DO UPDATE
                 SET status = 'active', checked_at = now(), version = app_licence.version + 1,
                     updated_at = now()",
        )
        .bind(tenant_id)
        .bind(crate::licence::DEFAULT_APP_KEY)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }

    // The platform-admin fixture (George 2026-11-09): one dev principal that is admin of
    // `kata` and `execution` and holds the global `platform_admin` grant. It is what the
    // Platform screen acts as, and what makes the tenant switcher exercisable locally.
    {
        let (subject, email, display_name) = DEV_PLATFORM_ADMIN;
        sqlx::query(
            "INSERT INTO app_user (oidc_subject, email, display_name)
             VALUES ($1, $2, $3)
             ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL DO NOTHING",
        )
        .bind(subject)
        .bind(email)
        .bind(display_name)
        .execute(pool)
        .await?;
        let user_id: Uuid = sqlx::query_scalar("SELECT id FROM app_user WHERE oidc_subject = $1")
            .bind(subject)
            .fetch_optional(pool)
            .await?
            .ok_or(SeedError::Missing("app_user", subject))?;
        for slug in DEV_PLATFORM_TENANTS {
            let tenant_id: Uuid = sqlx::query_scalar("SELECT id FROM tenant WHERE slug = $1")
                .bind(slug)
                .fetch_optional(pool)
                .await?
                .ok_or(SeedError::Missing("tenant", slug))?;
            let tenant = TenantId::from_uuid(tenant_id)?;
            let mut tx = begin_scoped(pool, tenant).await?;
            ensure_parent_tx(&mut tx, tenant, Parent::AppUser, user_id).await?;
            sqlx::query(
                "INSERT INTO membership (tenant_id, user_id, role)
                 VALUES ($1, $2, 'admin')
                 ON CONFLICT (tenant_id, user_id) DO NOTHING",
            )
            .bind(tenant_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO app_licence (tenant_id, app_key, status, checked_at)
                 VALUES ($1, $2, 'active', now())
                 ON CONFLICT (tenant_id, app_key) DO UPDATE
                     SET status = 'active', checked_at = now(),
                         version = app_licence.version + 1, updated_at = now()",
            )
            .bind(tenant_id)
            .bind(crate::licence::DEFAULT_APP_KEY)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
        crate::rbac::grant_platform_admin(pool, user_id, None).await?;
    }

    // The S16 usage fixture: dev-a gets a ledger, dev-b stays empty so the empty state is a
    // real tenant, not a mocked response.
    measure(pool).await?;

    // The improvement-screen fixtures (S16 audit, S17 controls, S19 eval) follow the same rule:
    // dev-a carries them, dev-b stays empty.
    measure_improve(pool).await?;

    // The catalogue gap S05 closes for development: without a `project_type` row every
    // `may_move` denies, and without a `board_template` row `projects.create` has no lanes to
    // create. S07 owns the FULL catalogue seed; this is the worked row plus one template.
    catalogue(pool).await?;

    Ok(())
}

/// The S16 usage fixture: twelve real ledger rows for `dev-a`, none for `dev-b`.
///
/// `caller_kind = 'other'` is deliberate. Every other kind must carry a run/turn join key
/// (`inference_call_join_key`), and the seed does not create a run just to label a fixture;
/// `other` is the schema's own "no join" value, not a fabricated reference.
///
/// Fixed ids make it idempotent; a second run refreshes `created_at` so the fixture stays
/// inside the 7-day window. That is the second deliberate rewrite in this module (the licence
/// row is the first), and it changes no other column.
async fn measure(pool: &PgPool) -> Result<(), SeedError> {
    struct Row {
        id: &'static str,
        prompt_ref: &'static str,
        prompt_version: &'static str,
        sha: &'static str,
        model: &'static str,
        prompt_tokens: i32,
        completion_tokens: i32,
        creation: i32,
        read: i32,
        status: &'static str,
        latency_ms: i32,
        hours_ago: i32,
    }

    // Three days x four calls. The classes are the point: `a` is read repeatedly, `b` is
    // written and never read (the cache-hostile prompt), `c` misses, and one call is
    // `unknown`. Token counts are plausible, not special.
    const A: &str = "7c2a5f0d9b1e4a638c2f7d0b6e9a1c3f5d8b2e4a7c0f9d1b3e6a8c2f4d7b9e1a";
    const B: &str = "91be3a6c0d2f5b8e1a4c7d0b3e6f9a2c5d8b1e4a7c0f2d5b8e3a6c9f1d4b7e2a";
    const C: &str = "3fd04b7e1a5c8d2f6b9e3a0c7d4f1b8e5a2c9d6f3b0e7a4c1d8f5b2e9a6c3d0f";
    const SONNET: &str = "claude-sonnet-5-5";
    const HAIKU: &str = "claude-haiku-5-5";
    const ROWS: [Row; 12] = [
        Row {
            id: "0195f4a0-0000-7000-8000-000000000001",
            prompt_ref: "copilot.system@1",
            prompt_version: "1",
            sha: A,
            model: SONNET,
            prompt_tokens: 3200,
            completion_tokens: 640,
            creation: 0,
            read: 2800,
            status: "hit",
            latency_ms: 920,
            hours_ago: 49,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000002",
            prompt_ref: "copilot.system@1",
            prompt_version: "1",
            sha: B,
            model: SONNET,
            prompt_tokens: 3000,
            completion_tokens: 500,
            creation: 2600,
            read: 0,
            status: "write",
            latency_ms: 1680,
            hours_ago: 46,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000003",
            prompt_ref: "process.review@2",
            prompt_version: "2",
            sha: C,
            model: HAIKU,
            prompt_tokens: 900,
            completion_tokens: 200,
            creation: 0,
            read: 0,
            status: "miss",
            latency_ms: 410,
            hours_ago: 43,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000004",
            prompt_ref: "process.review@2",
            prompt_version: "2",
            sha: C,
            model: HAIKU,
            prompt_tokens: 950,
            completion_tokens: 220,
            creation: 0,
            read: 0,
            status: "unknown",
            latency_ms: 430,
            hours_ago: 40,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000005",
            prompt_ref: "copilot.system@1",
            prompt_version: "1",
            sha: A,
            model: SONNET,
            prompt_tokens: 3300,
            completion_tokens: 700,
            creation: 0,
            read: 2900,
            status: "hit",
            latency_ms: 870,
            hours_ago: 31,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000006",
            prompt_ref: "copilot.system@1",
            prompt_version: "1",
            sha: B,
            model: SONNET,
            prompt_tokens: 3100,
            completion_tokens: 520,
            creation: 2700,
            read: 0,
            status: "write",
            latency_ms: 1740,
            hours_ago: 28,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000007",
            prompt_ref: "workflow.step@1",
            prompt_version: "1",
            sha: A,
            model: HAIKU,
            prompt_tokens: 1200,
            completion_tokens: 300,
            creation: 0,
            read: 1000,
            status: "hit",
            latency_ms: 520,
            hours_ago: 25,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000008",
            prompt_ref: "process.review@2",
            prompt_version: "2",
            sha: C,
            model: SONNET,
            prompt_tokens: 3400,
            completion_tokens: 650,
            creation: 0,
            read: 0,
            status: "miss",
            latency_ms: 1100,
            hours_ago: 22,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000009",
            prompt_ref: "copilot.system@1",
            prompt_version: "1",
            sha: A,
            model: SONNET,
            prompt_tokens: 3350,
            completion_tokens: 710,
            creation: 0,
            read: 2950,
            status: "hit",
            latency_ms: 840,
            hours_ago: 7,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000010",
            prompt_ref: "copilot.system@1",
            prompt_version: "1",
            sha: B,
            model: HAIKU,
            prompt_tokens: 1100,
            completion_tokens: 280,
            creation: 900,
            read: 0,
            status: "write",
            latency_ms: 610,
            hours_ago: 5,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000011",
            prompt_ref: "copilot.system@1",
            prompt_version: "1",
            sha: A,
            model: SONNET,
            prompt_tokens: 3250,
            completion_tokens: 690,
            creation: 0,
            read: 2850,
            status: "hit",
            latency_ms: 860,
            hours_ago: 3,
        },
        Row {
            id: "0195f4a0-0000-7000-8000-000000000012",
            prompt_ref: "process.review@2",
            prompt_version: "2",
            sha: C,
            model: SONNET,
            prompt_tokens: 3450,
            completion_tokens: 660,
            creation: 0,
            read: 0,
            status: "miss",
            latency_ms: 1180,
            hours_ago: 1,
        },
    ];

    let tenant_id: Uuid = sqlx::query_scalar("SELECT id FROM tenant WHERE slug = 'dev-a'")
        .fetch_optional(pool)
        .await?
        .ok_or(SeedError::Missing("tenant", "dev-a"))?;
    let tenant = TenantId::from_uuid(tenant_id)?;
    let mut tx = begin_scoped(pool, tenant).await?;
    for row in ROWS {
        sqlx::query(
            "INSERT INTO inference_call
                 (id, tenant_id, caller_kind, prompt_ref, prompt_version, prompt_sha256,
                  model, served_model, prompt_tokens, completion_tokens, total_tokens,
                  cache_creation_input_tokens, cache_read_input_tokens, cache_status,
                  latency_ms, outcome, created_at)
             VALUES ($1::uuid, $2, 'other', $3, $4, $5, $6, $6, $7, $8, $7 + $8,
                     $9, $10, $11, $12, 'ok',
                     now() - ($13::int * interval '1 hour'))
             ON CONFLICT (id) DO UPDATE SET created_at = EXCLUDED.created_at",
        )
        .bind(row.id)
        .bind(tenant_id)
        .bind(row.prompt_ref)
        .bind(row.prompt_version)
        .bind(row.sha)
        .bind(row.model)
        .bind(row.prompt_tokens)
        .bind(row.completion_tokens)
        .bind(row.creation)
        .bind(row.read)
        .bind(row.status)
        .bind(row.latency_ms)
        .bind(row.hours_ago)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The improvement-screen fixtures for `dev-a`: a finished prompt audit with four findings and
/// one queued audit, the seeded benchmark suite with two finished control runs, and one eval
/// with a version, cases, metrics, a drawn split, one approved and one pending sign-off, one
/// completed run with results, and a planned climb.
///
/// `dev-b` stays empty on purpose — the empty states are exercised on a real tenant.
///
/// Idempotence: every row carries a fixed id and `ON CONFLICT DO NOTHING`. Three deliberate
/// rewrites, each because the fixture would otherwise lie: the eval version's `digest` and the
/// approved sign-off's `artifact_sha256` are recomputed from the seeded rows on every run (an
/// edited fixture must not present a sign-off for an instrument nobody signed), and the usage
/// fixture's `created_at` (see [`measure`]).
async fn measure_improve(pool: &PgPool) -> Result<(), SeedError> {
    let tenant_id: Uuid = sqlx::query_scalar("SELECT id FROM tenant WHERE slug = 'dev-a'")
        .fetch_optional(pool)
        .await?
        .ok_or(SeedError::Missing("tenant", "dev-a"))?;
    let tenant = TenantId::from_uuid(tenant_id)?;
    let alice: Uuid =
        sqlx::query_scalar("SELECT id FROM app_user WHERE oidc_subject = 'dev:alice'")
            .fetch_optional(pool)
            .await?
            .ok_or(SeedError::Missing("app_user", "dev:alice"))?;

    // The suite is the embedded artifact; idempotent by `seed_sha256`, so a second call is a
    // no-op. It is normally reached only through `--seed-catalogue`; the screens need it on a
    // plain `--dev-seed` too.
    crate::seed_benchmark::seed_benchmark(pool, Some("dev-a")).await?;

    let mut tx = begin_scoped(pool, tenant).await?;
    seed_prompt_audit(&mut tx, tenant_id, alice).await?;
    seed_benchmark_controls(&mut tx, tenant_id).await?;
    seed_eval(&mut tx, tenant, tenant_id, alice).await?;
    tx.commit().await?;
    Ok(())
}

/// One finished audit and one queued audit for `dev-a`.
async fn seed_prompt_audit(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    alice: Uuid,
) -> Result<(), SeedError> {
    struct Finding {
        id: &'static str,
        location: &'static str,
        evidence: &'static str,
        pattern: &'static str,
        why_obsolete: &'static str,
        confidence: &'static str,
        action: &'static str,
        replacement: Option<&'static str>,
        status: &'static str,
        decided: bool,
    }

    const FINDINGS: [Finding; 4] = [
        Finding {
            id: "0195f4b1-0000-7000-8000-000000000001",
            location: "process bug-fix · step 2",
            evidence: "Apply the patch to the repository using the tools available.",
            pattern: "1a",
            why_obsolete: "The template restates what completions.create already says, so the model receives the same instruction twice in different words and trusts the second copy.",
            confidence: "high",
            action: "rewrite",
            replacement: Some("Apply the patch. Return the diff and the test log."),
            status: "open",
            decided: false,
        },
        Finding {
            id: "0195f4b1-0000-7000-8000-000000000002",
            location: "copilot system prompt",
            evidence: "Use the cards.create action when you need a new card.",
            pattern: "3",
            why_obsolete: "The description names a field that was removed from the schema two migrations ago; the model still tries to send it in about 4% of calls.",
            confidence: "medium",
            action: "flag",
            replacement: None,
            status: "open",
            decided: false,
        },
        Finding {
            id: "0195f4b1-0000-7000-8000-000000000003",
            location: "process review · description",
            evidence: "You are an AI assistant that is very helpful and always careful.",
            pattern: "2",
            why_obsolete: "Twenty-two tokens of framing precede the first instruction, which sits after the point where the model has usually already chosen a tool.",
            confidence: "low",
            action: "remove",
            replacement: None,
            status: "accepted",
            decided: true,
        },
        Finding {
            id: "0195f4b1-0000-7000-8000-000000000004",
            location: "process bug-fix · step 5",
            evidence: "Ask the user for permission before running the test suite.",
            pattern: "4",
            why_obsolete: "The gate already asks once; a second prompt inside the step stalls runs that could finish.",
            confidence: "medium",
            action: "remove",
            replacement: None,
            status: "rejected",
            decided: true,
        },
    ];

    sqlx::query(
        "INSERT INTO prompt_audit
             (id, tenant_id, scope, target_model, prompt_ref, status, finding_count,
              requested_by, created_at, updated_at)
         VALUES ($1::uuid, $2, $3, 'claude-haiku-5-5', 'prompt_audit@1', 'succeeded', 4,
                 $4, now() - interval '2 hours', now() - interval '2 hours')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind("0195f4b0-0000-7000-8000-000000000001")
    .bind(tenant_id)
    .bind(serde_json::json!({"surfaces": ["processes", "copilot"], "process_ids": []}))
    .bind(alice)
    .execute(&mut *conn)
    .await?;

    for finding in FINDINGS {
        sqlx::query(
            "INSERT INTO prompt_audit_finding
                 (id, tenant_id, audit_id, location, evidence, pattern, why_obsolete, confidence,
                  action, replacement, status, decided_by, decided_at, created_at, updated_at)
             VALUES ($1::uuid, $2, $3::uuid, $4, $5, $6, $7, $8, $9, $10, $11,
                     CASE WHEN $12 THEN $13::uuid END,
                     CASE WHEN $12 THEN now() - interval '1 hour' END,
                     now() - interval '2 hours', now() - interval '1 hour')
             ON CONFLICT (id) DO UPDATE
                 SET status = EXCLUDED.status,
                     decided_by = EXCLUDED.decided_by,
                     decided_at = EXCLUDED.decided_at,
                     updated_at = now()",
        )
        .bind(finding.id)
        .bind(tenant_id)
        .bind("0195f4b0-0000-7000-8000-000000000001")
        .bind(finding.location)
        .bind(finding.evidence)
        .bind(finding.pattern)
        .bind(finding.why_obsolete)
        .bind(finding.confidence)
        .bind(finding.action)
        .bind(finding.replacement)
        .bind(finding.status)
        .bind(finding.decided)
        .bind(alice)
        .execute(&mut *conn)
        .await?;
    }

    // One queued audit so the running state is visible without waiting on a model call.
    sqlx::query(
        "INSERT INTO prompt_audit
             (id, tenant_id, scope, target_model, prompt_ref, status, finding_count,
              requested_by, created_at, updated_at)
         VALUES ('0195f4b0-0000-7000-8000-000000000002'::uuid, $1, $2, 'claude-haiku-5-5',
                 'prompt_audit@1', 'queued', 0, $3, now() - interval '3 minutes',
                 now() - interval '3 minutes')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(serde_json::json!({"surfaces": ["processes"], "process_ids": []}))
    .bind(alice)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The seeded suite plus two finished control runs (gold resolves all, empty none) with eight
/// case rows each. No baseline or conveyor run is seeded: those arms are refused by the action
/// until S18, and a fixture that produced them would be the exact fake the screen refuses.
async fn seed_benchmark_controls(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
) -> Result<(), SeedError> {
    let suite_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM benchmark_suite WHERE tenant_id = $1 AND slug = 'verified50-v1'",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(SeedError::Missing("benchmark_suite", "verified50-v1"))?;

    let instances: Vec<(String,)> = sqlx::query_as(
        "SELECT instance_id FROM benchmark_instance
          WHERE tenant_id = $1 AND suite_id = $2
          ORDER BY instance_id
          LIMIT 8",
    )
    .bind(tenant_id)
    .bind(suite_id)
    .fetch_all(&mut *conn)
    .await?;
    if instances.len() < 8 {
        return Err(SeedError::Missing(
            "benchmark_instance",
            "eight suite instances",
        ));
    }

    let runs = [
        (
            "0195f4c0-0000-7000-8000-000000000001",
            "gold",
            "gold",
            8i32,
            serde_json::json!({"resolved": 8}),
        ),
        (
            "0195f4c0-0000-7000-8000-000000000002",
            "empty",
            "empty",
            0i32,
            serde_json::json!({"unresolved": 8}),
        ),
    ];
    for (id, arm, label, resolved, counts) in runs {
        sqlx::query(
            "INSERT INTO benchmark_run
                 (id, tenant_id, suite_id, arm, subject_kind, label, model, status, n, n_graded,
                  n_resolved, n_no_attempt, counts, control_ok, last_polled_at, started_at,
                  finished_at, environment, options, created_at, updated_at)
             VALUES ($1::uuid, $2, $3, $4, 'suite', $5, 'claude-sonnet-5-5', 'succeeded', 8, 8,
                     $6, 0, $7, true, now() - interval '2 hours',
                     now() - interval '3 hours', now() - interval '2 hours', '{}', '{}',
                     now() - interval '3 hours', now() - interval '2 hours')
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(suite_id)
        .bind(arm)
        .bind(label)
        .bind(resolved)
        .bind(&counts)
        .execute(&mut *conn)
        .await?;

        let case_prefix = if arm == "gold" {
            "0195f4c1-0000-7000-8000-"
        } else {
            "0195f4c2-0000-7000-8000-"
        };
        let verdict = if arm == "gold" {
            "resolved"
        } else {
            "unresolved"
        };
        for (index, (instance,)) in instances.iter().enumerate() {
            let case_id = format!("{case_prefix}{:012}", index + 1);
            sqlx::query(
                "INSERT INTO benchmark_case
                     (id, tenant_id, benchmark_run_id, instance_id, produce_status, attempts,
                      verdict, seconds, detail, created_at, updated_at)
                 VALUES ($1::uuid, $2, $3::uuid, $4, 'ok', 1, $5,
                         $6::numeric, '{}', now() - interval '3 hours', now() - interval '2 hours')
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(&case_id)
            .bind(tenant_id)
            .bind(id)
            .bind(instance)
            .bind(verdict)
            .bind(45.0 + (index as f64) * 17.5)
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

/// One eval instrument (version, six cases, two metrics, a drawn split, one approved and one
/// pending sign-off), one completed baseline run with a result per case, and one planned climb.
async fn seed_eval(
    conn: &mut sqlx::PgConnection,
    tenant: TenantId,
    tenant_id: Uuid,
    alice: Uuid,
) -> Result<(), SeedError> {
    const EVAL_ID: &str = "0195f4d0-0000-7000-8000-000000000001";
    const VERSION_ID: &str = "0195f4d0-0000-7000-8000-000000000002";
    const RUN_ID: &str = "0195f4d0-0000-7000-8000-000000000003";
    const CLIMB_ID: &str = "0195f4d0-0000-7000-8000-000000000004";

    let cases: [(&str, &str, &str); 6] = [
        (
            "refund duplicate",
            "payments",
            "When a customer is charged twice for one order, find the duplicate and say how you would refund it.",
        ),
        (
            "webhook ordering",
            "payments",
            "Two webhooks for the same event arrive out of order. Explain how you decide which one wins.",
        ),
        (
            "retry storm",
            "infra",
            "A retry storm doubles traffic every minute. Name the first mitigation and why it comes first.",
        ),
        (
            "audit actor kind",
            "security",
            "An audit row must name the actor kind. Which kinds are legal and which one is missing?",
        ),
        (
            "idempotent replay",
            "payments",
            "A request is replayed with the same idempotency key. What must the server return?",
        ),
        (
            "held-out split",
            "infra",
            "Why must a test split stay unknown while the method is being tuned?",
        ),
    ];

    sqlx::query(
        "INSERT INTO eval
             (id, tenant_id, name, purpose, subject_kind, entry_action, model_ref, prompt_ref,
              inputs_schema, trace_source, status, current_version_id, created_by, created_at,
              updated_at)
         VALUES ($1::uuid, $2, 'bug-fix judge',
                 'Does the flow choose a correct, grounded fix?',
                 'completions', 'inference.chat', 'claude-sonnet-5-5', 'agent@1',
                 '{}', 'synthesized', 'draft', $3::uuid, $4,
                 now() - interval '1 day', now() - interval '1 day')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(EVAL_ID)
    .bind(tenant_id)
    .bind(VERSION_ID)
    .bind(alice)
    .execute(&mut *conn)
    .await?;

    // `digest` is NOT NULL with no default: seed a placeholder, then replace it with the store's
    // own digest after the instrument's rows exist, so readiness and the sign-off tab agree.
    sqlx::query(
        "INSERT INTO eval_version
             (id, tenant_id, eval_id, version, digest, prices, harness_paths, created_by,
              created_at)
         VALUES ($1::uuid, $2, $3::uuid, 1, 'seed-placeholder',
                 '{\"input\": 3, \"output\": 15}', '{}', $4, now() - interval '1 day')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(VERSION_ID)
    .bind(tenant_id)
    .bind(EVAL_ID)
    .bind(alice)
    .execute(&mut *conn)
    .await?;

    for (ordinal, (_component, tag, input)) in cases.iter().enumerate() {
        let case_id = format!("0195f4d1-0000-7000-8000-{:012}", ordinal + 1);
        sqlx::query(
            "INSERT INTO eval_case
                 (id, tenant_id, version_id, ordinal, input, tags, source, gold_provenance,
                  split, created_at, updated_at)
             VALUES ($1::uuid, $2, $3::uuid, $4, $5, ARRAY[$6]::text[], 'handwritten',
                     'human_written', 'none', now() - interval '1 day',
                     now() - interval '1 day')
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&case_id)
        .bind(tenant_id)
        .bind(VERSION_ID)
        .bind(ordinal as i32)
        .bind(*input)
        .bind(*tag)
        .execute(&mut *conn)
        .await?;
    }

    for (ordinal, (key, role, kind, label, grader_kind, grader)) in [
        (
            "correct",
            "quality",
            "binary",
            "correct",
            "programmatic",
            serde_json::json!({"expect": "exact"}),
        ),
        (
            "grounded",
            "quality",
            "judge",
            "grounded",
            "rubric",
            serde_json::json!({}),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query(
            "INSERT INTO eval_metric
                 (id, tenant_id, version_id, key, role, kind, better, label, grader_kind,
                  grader, rubric, ordinal, created_at)
             VALUES ($1::uuid, $2, $3::uuid, $4, $5, $6, 'higher', $7, $8, $9,
                     'The answer names the right fix and the evidence it relies on.', $10,
                     now() - interval '1 day')
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(format!("0195f4d2-0000-7000-8000-{:012}", ordinal + 1))
        .bind(tenant_id)
        .bind(VERSION_ID)
        .bind(key)
        .bind(role)
        .bind(kind)
        .bind(label)
        .bind(grader_kind)
        .bind(grader)
        .bind(ordinal as i32)
        .execute(&mut *conn)
        .await?;
    }

    // The instrument digest, from the store's one implementation (never re-implemented here).
    let digest = crate::eval::version_digest_tx(
        &mut *conn,
        tenant,
        Uuid::parse_str(VERSION_ID).map_err(|error| SeedError::Eval(error.to_string()))?,
    )
    .await
    .map_err(|error| SeedError::Eval(error.to_string()))?;
    sqlx::query("UPDATE eval_version SET digest = $2 WHERE tenant_id = $1 AND id = $3::uuid")
        .bind(tenant_id)
        .bind(&digest)
        .bind(VERSION_ID)
        .execute(&mut *conn)
        .await?;

    // Draw and lock the split exactly as `evals.draw_split` does (same seed = the digest).
    let split_sha: Option<String> = sqlx::query_scalar(
        "SELECT split_sha256 FROM eval_version WHERE tenant_id = $1 AND id = $2::uuid",
    )
    .bind(tenant_id)
    .bind(VERSION_ID)
    .fetch_one(&mut *conn)
    .await?;
    if split_sha.is_none() {
        let rows = crate::eval::cases_for_split_tx(
            &mut *conn,
            tenant,
            Uuid::parse_str(VERSION_ID).map_err(|error| SeedError::Eval(error.to_string()))?,
        )
        .await
        .map_err(|error| SeedError::Eval(error.to_string()))?;
        let split_cases: Vec<conveyor_domain::eval::SplitCase<'_>> = rows
            .iter()
            .map(|(id, tags)| conveyor_domain::eval::SplitCase {
                id: *id,
                tags: tags.as_slice(),
            })
            .collect();
        let assignment = conveyor_domain::eval::draw_split(&split_cases, 0.7, digest.as_bytes())
            .map_err(|error| SeedError::Eval(error.to_string()))?;
        crate::eval::lock_split_tx(
            &mut *conn,
            tenant,
            Uuid::parse_str(VERSION_ID).map_err(|error| SeedError::Eval(error.to_string()))?,
            &assignment.split_sha256,
            &assignment.per_case,
        )
        .await
        .map_err(|error| SeedError::Eval(error.to_string()))?;
    }

    // Both sign-off rows: `eval_inputs` approved (so the tab shows a real decision), and
    // `eval_grading` pending (so Run start refuses and names the gate).
    for (id, gate, status) in [
        (
            "0195f4d3-0000-7000-8000-000000000001",
            "eval_inputs",
            "approved",
        ),
        (
            "0195f4d3-0000-7000-8000-000000000002",
            "eval_grading",
            "pending",
        ),
    ] {
        sqlx::query(
            "INSERT INTO approval
                 (id, tenant_id, eval_id, gate, kind, status, required_decisions, requested_by,
                  meta, created_at, updated_at)
             VALUES ($1::uuid, $2, $3::uuid, $4, 'eval', $5, 1, $6,
                     jsonb_build_object('artifact_sha256', $7, 'ask', $8),
                     now() - interval '20 hours', now() - interval '20 hours')
             ON CONFLICT (id) DO UPDATE
                 SET meta = EXCLUDED.meta, status = EXCLUDED.status, updated_at = now()",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(EVAL_ID)
        .bind(gate)
        .bind(status)
        .bind(alice)
        .bind(&digest)
        .bind(match gate {
            "eval_inputs" => "The six cases and their golds are the instrument to measure.",
            _ => "The grading rubric is the instrument; approve it to run.",
        })
        .execute(&mut *conn)
        .await?;
    }
    // The grading gate is the fixture's open question. A re-seed resets its status, so any
    // decision a previous session recorded must go too — a pending gate with a decision row
    // is a state the product cannot produce, and it would refuse the next approve as
    // AlreadyDecided.
    sqlx::query(
        "DELETE FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = '0195f4d3-0000-7000-8000-000000000002'::uuid",
    )
    .bind(tenant_id)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO approval_decision
             (id, tenant_id, approval_id, approver_id, decision, note, decided_by_kind, created_at)
         VALUES ('0195f4d4-0000-7000-8000-000000000001'::uuid, $1,
                 '0195f4d3-0000-7000-8000-000000000001'::uuid, $2, 'approve',
                 'the inputs are the instrument', 'human', now() - interval '19 hours')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(alice)
    .execute(&mut *conn)
    .await?;

    // One completed baseline run and a result per case. The run is a record of what a driver
    // produced; it does not require the instrument to be ready now (the gate is on start).
    sqlx::query(
        "INSERT INTO eval_run
             (id, tenant_id, eval_id, version_id, variant, round, mode, reps, case_count,
              status, environment, summary, started_at, finished_at, created_at)
         VALUES ($1::uuid, $2, $3::uuid, $4::uuid, 'baseline', 0, 'full', 1, 6, 'complete',
                 '{}', '{}', now() - interval '50 minutes', now() - interval '40 minutes',
                 now() - interval '55 minutes')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(RUN_ID)
    .bind(tenant_id)
    .bind(EVAL_ID)
    .bind(VERSION_ID)
    .execute(&mut *conn)
    .await?;

    let scores = [
        (1.0_f64, 0.9_f64),
        (0.0, 0.4),
        (1.0, 1.0),
        (0.5, 0.7),
        (1.0, 0.8),
        (0.0, 0.3),
    ];
    for (ordinal, (correct, grounded)) in scores.iter().enumerate() {
        let case_id = format!("0195f4d1-0000-7000-8000-{:012}", ordinal + 1);
        let result_id = format!("0195f4d5-0000-7000-8000-{:012}", ordinal + 1);
        sqlx::query(
            "INSERT INTO eval_case_result
                 (id, tenant_id, eval_run_id, case_id, rep, status, grade, explanation, model,
                  usage, latency_ms, tool_calls, created_at)
             VALUES ($1::uuid, $2, $3::uuid, $4::uuid, 0, 'ok', $5, '{}',
                     'claude-sonnet-5-5', $6, $7, 0, now() - interval '45 minutes')
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(&result_id)
        .bind(tenant_id)
        .bind(RUN_ID)
        .bind(&case_id)
        .bind(serde_json::json!({"correct": correct, "grounded": grounded}))
        .bind(serde_json::json!({
            "prompt_tokens": 1200 + ordinal * 90,
            "completion_tokens": 280 + ordinal * 20,
            "total_tokens": 1480 + ordinal * 110
        }))
        .bind(2200 + (ordinal as i32) * 300)
        .execute(&mut *conn)
        .await?;
    }

    sqlx::query(
        "INSERT INTO eval_climb
             (id, tenant_id, eval_id, version_id, goal, direction, reps, stopping, plateau_k,
              max_rounds, delta, off_limits, approve_each_round, current_round, status,
              created_by, created_at, updated_at)
         VALUES ($1::uuid, $2, $3::uuid, $4::uuid,
                 '{\"target\": \"correct\", \"direction\": \"higher\", \"hold\": [], \"lever\": 0}',
                 'higher', 1, 'plateau', 3, 10, 0.05, ARRAY['grading'], true, 0, 'planned',
                 $5, now() - interval '18 hours', now() - interval '18 hours')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(CLIMB_ID)
    .bind(tenant_id)
    .bind(EVAL_ID)
    .bind(VERSION_ID)
    .bind(alice)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

///
/// `project_type` → card_type → from-kind → allowed to-kinds. An absent entry denies.
pub const DELIVERY_MATRIX: &str = r#"{
  "feature": {
    "intake": ["active", "archive"],
    "active": ["waiting", "done", "archive"],
    "waiting": ["active", "archive"],
    "done": ["archive"],
    "archive": ["intake"]
  },
  "bug": {
    "intake": ["active"],
    "active": ["done", "archive"],
    "done": ["archive"]
  }
}"#;

/// The default board template: one lane per `list.kind`, in lifecycle order.
pub const DEFAULT_BOARD_TEMPLATE: [&str; 5] = ["intake", "active", "waiting", "done", "archive"];

/// Insert the catalogue rows development needs, for every dev tenant.
///
/// Idempotent: every statement is `ON CONFLICT DO NOTHING`, so a second `--dev-seed` leaves
/// the rows byte-identical (the licence row in [`dev`] is the one deliberate exception).
///
/// It seeds three things, not one, because the first is unusable without the others:
///
/// 1. the worked `delivery` `project_type` row carrying `meta.matrix` (S03 §5);
/// 2. the `feature` and `bug` `card_type` rows the matrix is keyed by — a matrix whose card
///    types do not exist denies every move, and `cards.create` could not name a type;
/// 3. the `standard` `board_template` row whose `meta.lists` is the five lane kinds
///    (`projects.create` creates exactly these lanes).
pub async fn catalogue(pool: &PgPool) -> Result<(), SeedError> {
    let matrix: serde_json::Value = serde_json::from_str(DELIVERY_MATRIX)
        .map_err(|error| SeedError::Catalogue(error.to_string()))?;
    let template = serde_json::json!({ "lists": DEFAULT_BOARD_TEMPLATE });
    for (slug, _) in DEV_TENANTS {
        // A test fixture may seed only one of the two dev tenants; an absent one is skipped,
        // not an error (the seed is idempotent, not mandatory).
        let Some(tenant_id): Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM tenant WHERE slug = $1")
                .bind(slug)
                .fetch_optional(pool)
                .await?
        else {
            continue;
        };
        let tenant = TenantId::from_uuid(tenant_id)?;
        let mut tx = begin_scoped(pool, tenant).await?;

        sqlx::query(
            "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta, sort)
             VALUES ($1, 'project_type', 'delivery', 'Delivery', $2, 0)
             ON CONFLICT (tenant_id, kind, code) DO NOTHING",
        )
        .bind(tenant_id)
        .bind(serde_json::json!({ "matrix": matrix }))
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta, sort)
             VALUES ($1, 'board_template', 'standard', 'Standard board', $2, 0)
             ON CONFLICT (tenant_id, kind, code) DO NOTHING",
        )
        .bind(tenant_id)
        .bind(&template)
        .execute(&mut *tx)
        .await?;

        // `projects.create` starts every project with a `project_start_request_analysis` card
        // (S13 §5.8), so the dev/test catalogue must carry that type — a tenant seeded without it
        // cannot create a project at all.
        for (code, label) in [
            ("feature", "Feature"),
            ("bug", "Bug"),
            (
                "project_start_request_analysis",
                "Project start request analysis",
            ),
        ] {
            sqlx::query(
                "INSERT INTO card_type (tenant_id, code, label)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (tenant_id, code) DO NOTHING",
            )
            .bind(tenant_id)
            .bind(code)
            .bind(label)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        // SPEC D27: every seeded tenant gets the built-in prompt catalogue, so a fresh dev
        // database can run a resolved executor (and the copilot) without a manual seed.
        prompts_for_tenant(pool, tenant_id).await?;
    }
    Ok(())
}

/// One built-in prompt section: `(position, source_kind, required, template)`.
type PromptSeedSection = (i32, &'static str, bool, &'static str);

/// The built-in prompt catalogue (SPEC D27): `(key, version, title, sections)`.
///
/// `system` sections carry their text; the other kinds are rendered from a run's facts, so their
/// template is empty. `agent@1` makes the process and capability components required — an
/// executor that cannot see its step or its capability must refuse before it spends a call — and
/// leaves the role, card and knowledge optional, because a run may have none of them.
const BUILTIN_PROMPTS: &[(&str, i32, &str, &[PromptSeedSection])] = &[
    (
        "copilot_system",
        1,
        "Copilot system prompt",
        &[(1, "system", true, crate::prompts::COPILOT_SYSTEM_V1)],
    ),
    (
        "prompt_audit",
        1,
        "Prompt audit instruction",
        &[(1, "system", true, crate::prompts::PROMPT_AUDIT_V1)],
    ),
    (
        "agent",
        1,
        "Capability agent",
        &[
            (1, "system", true, crate::prompts::AGENT_V1),
            (2, "process", true, ""),
            (3, "capability", true, ""),
            // The binding's declared perspective (`capability_agent.params->>'role'`). Optional:
            // an agent that declares no role compiles exactly as before.
            (4, "role", false, ""),
            (5, "knowledge", false, ""),
            (6, "project_card", false, ""),
        ],
    ),
];

/// Seed the built-in prompts for one tenant (SPEC D27).
///
/// Idempotent **and additive**: an existing `(key, version)` is left exactly as it is, because a
/// seed must never overwrite a tenant's prompt — the row is the truth once it exists, and a change
/// is a new version, not an edit.
pub async fn prompts_for_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<(), SeedError> {
    let tenant = TenantId::from_uuid(tenant_id)?;
    let mut tx = begin_scoped(pool, tenant).await?;
    for (key, version, title, sections) in BUILTIN_PROMPTS {
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO prompt (tenant_id, key, version, status, title)
             VALUES ($1, $2, $3, 'active', $4)
             ON CONFLICT (tenant_id, key, version) DO NOTHING
             RETURNING id",
        )
        .bind(tenant_id)
        .bind(*key)
        .bind(*version)
        .bind(*title)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(prompt_id) = inserted else {
            continue;
        };
        for (position, source_kind, required, template) in *sections {
            sqlx::query(
                "INSERT INTO prompt_section
                     (tenant_id, prompt_id, position, source_kind, required, template)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(tenant_id)
            .bind(prompt_id)
            .bind(*position)
            .bind(*source_kind)
            .bind(*required)
            .bind(*template)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

// ── S07: the checked-in catalogue export ─────────────────────────────────────────────────
/// The catalogue export, embedded at compile time so a deployment carries it even when
/// `seed/` is absent from the image. Read-only, one-time export from RM (`generated_from` in
/// the document); it is imported, never regenerated here.
const CATALOGUE_EXPORT: &str = include_str!("../../../seed/catalogue.json");

/// The document as [`CatalogueExport`]. Only the two row arrays are modelled; the header
/// fields (`version`, `generated_from`, `source_note`, `matrix_note`) are documentation and
/// serde ignores them.
#[derive(Deserialize)]
struct CatalogueDocument {
    #[serde(default)]
    card_types: Vec<CardTypeDocument>,
    #[serde(default)]
    entries: Vec<EntryDocument>,
}

#[derive(Deserialize)]
struct EntryDocument {
    kind: String,
    code: String,
    label: String,
    #[serde(default = "empty_object")]
    meta: Value,
    #[serde(default)]
    sort: i32,
    #[serde(default = "active_by_default")]
    is_active: bool,
}

#[derive(Deserialize)]
struct CardTypeDocument {
    code: String,
    label: String,
    #[serde(default = "empty_object")]
    definition: Value,
    /// The export omits `is_active` on every card type; absent means offered.
    #[serde(default = "active_by_default")]
    is_active: bool,
}

fn active_by_default() -> bool {
    true
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// The parsed contents of `seed/catalogue.json`, ready for the importer.
#[derive(Clone, Debug)]
pub struct CatalogueExport {
    /// The `catalogue_entry` rows.
    pub entries: Vec<ImportEntry>,
    /// The `card_type` rows — imported FIRST, because a project type's matrix is keyed by
    /// their codes.
    pub card_types: Vec<CardTypeImport>,
}

static EXPORT: OnceLock<CatalogueExport> = OnceLock::new();

/// The embedded catalogue export, parsed once and cloned per call.
///
/// The document is a build-time constant, so a parse failure is a programming error surfaced
/// as [`SeedError::Catalogue`], never a panic and never a runtime data error.
pub fn catalogue_export() -> Result<CatalogueExport, SeedError> {
    if let Some(export) = EXPORT.get() {
        return Ok(export.clone());
    }
    let export = parse_catalogue_export()?;
    // A concurrent first caller may win the slot; both parsed the same constant, so the
    // loser's value is identical and is simply dropped.
    let _ = EXPORT.set(export.clone());
    Ok(export)
}

fn parse_catalogue_export() -> Result<CatalogueExport, SeedError> {
    let document: CatalogueDocument = serde_json::from_str(CATALOGUE_EXPORT)
        .map_err(|error| SeedError::Catalogue(error.to_string()))?;
    Ok(CatalogueExport {
        entries: document
            .entries
            .into_iter()
            .map(|entry| ImportEntry {
                kind: entry.kind,
                code: entry.code,
                label: entry.label,
                meta: entry.meta,
                sort: entry.sort,
                is_active: entry.is_active,
            })
            .collect(),
        card_types: document
            .card_types
            .into_iter()
            .map(|card_type| CardTypeImport {
                code: card_type.code,
                label: card_type.label,
                definition: card_type.definition,
                is_active: card_type.is_active,
            })
            .collect(),
    })
}

// ── S15: the capability document ─────────────────────────────────────────────────────────

/// The capability seed, embedded at compile time. The 17 rows moved here from
/// `seed/catalogue.json` in S15, because a capability is an aggregate, not a catalogue kind.
const CAPABILITY_EXPORT: &str = include_str!("../../../seed/capability.json");

#[derive(Deserialize)]
struct CapabilityDocument {
    #[serde(default)]
    capabilities: Vec<CapabilityDocumentRow>,
    #[serde(default)]
    relations: Vec<RelationDocumentRow>,
}

#[derive(Deserialize)]
struct CapabilityDocumentRow {
    code: String,
    label: String,
    #[serde(default)]
    description: String,
    #[serde(default = "draft_status")]
    status: String,
    #[serde(default = "empty_object")]
    meta: Value,
}

#[derive(Deserialize)]
struct RelationDocumentRow {
    from: String,
    to: String,
    kind: String,
}

fn draft_status() -> String {
    "draft".to_owned()
}

/// The parsed contents of `seed/capability.json`.
#[derive(Clone, Debug)]
pub struct CapabilitySeedExport {
    /// The capability rows, in document order.
    pub capabilities: Vec<CapabilityImport>,
    /// The relation edges, by capability code.
    pub relations: Vec<RelationImport>,
}

static CAPABILITY_SEED: OnceLock<CapabilitySeedExport> = OnceLock::new();

/// The embedded capability export, parsed once and cloned per call.
pub fn capability_export() -> Result<CapabilitySeedExport, SeedError> {
    if let Some(export) = CAPABILITY_SEED.get() {
        return Ok(export.clone());
    }
    let export = parse_capability_export()?;
    let _ = CAPABILITY_SEED.set(export.clone());
    Ok(export)
}

fn parse_capability_export() -> Result<CapabilitySeedExport, SeedError> {
    let document: CapabilityDocument = serde_json::from_str(CAPABILITY_EXPORT)
        .map_err(|error| SeedError::CapabilityDocument(error.to_string()))?;
    Ok(CapabilitySeedExport {
        capabilities: document
            .capabilities
            .into_iter()
            .map(|row| CapabilityImport {
                code: row.code,
                label: row.label,
                description: row.description,
                status: row.status,
                meta: row.meta,
            })
            .collect(),
        relations: document
            .relations
            .into_iter()
            .map(|row| RelationImport {
                from_code: row.from,
                to_code: row.to,
                kind: row.kind,
            })
            .collect(),
    })
}

/// Import the embedded catalogue export into one tenant, or into every active tenant.
///
/// `Some(slug)` targets exactly that tenant and an unknown slug is [`SeedError::UnknownTenant`]
/// naming it; `None` targets every row of [`tenants::active_ids`], each re-read to its slug.
///
/// One [`begin_scoped`] transaction per tenant, importing the card types before the entries so
/// a project type's `meta.card_types` codes resolve, then committed. The importer is
/// idempotent, so a second run reports `{created: 0, updated: 0, skipped: n}` and rewrites
/// nothing; the returned counts are summed over both imports.
pub async fn seed_catalogue(
    pool: &PgPool,
    tenant_slug: Option<&str>,
) -> Result<Vec<(String, ImportCounts)>, SeedError> {
    let export = catalogue_export()?;
    let capability_seed = capability_export()?;
    let targets = match tenant_slug {
        Some(slug) => {
            let tenant_id: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM tenant WHERE slug = $1")
                    .bind(slug)
                    .fetch_optional(pool)
                    .await?;
            let tenant_id = tenant_id.ok_or_else(|| SeedError::UnknownTenant(slug.to_owned()))?;
            vec![(tenant_id, slug.to_owned())]
        }
        None => {
            let mut targets = Vec::new();
            for tenant in tenants::active_ids(pool).await? {
                let slug: Option<String> =
                    sqlx::query_scalar("SELECT slug FROM tenant WHERE id = $1")
                        .bind(tenant.as_uuid())
                        .fetch_optional(pool)
                        .await?;
                // `active_ids` and this re-read are two statements; a row both read cannot
                // lack a slug, so an absent one is a vanished tenant, not a typo.
                let slug = slug.ok_or(SeedError::Missing("tenant", "active tenant re-read"))?;
                targets.push((tenant.as_uuid(), slug));
            }
            targets
        }
    };

    let mut results = Vec::with_capacity(targets.len());
    for (tenant_id, slug) in targets {
        let tenant = TenantId::from_uuid(tenant_id)?;
        let mut tx = begin_scoped(pool, tenant).await?;
        let card_types = import_card_types_tx(&mut tx, tenant, &export.card_types).await?;
        let entries = import_entries_tx(&mut tx, tenant, &export.entries).await?;
        let capability_counts =
            import_capabilities_tx(&mut tx, tenant, &capability_seed.capabilities).await?;
        let relation_counts =
            import_relations_tx(&mut tx, tenant, &capability_seed.relations).await?;
        // SPEC D20/D29: the fixed system capabilities. The import cannot know which of RM's
        // vocabulary is system, so it is asserted here, after the rows exist — a later seed run
        // (or a tenant edit) that cleared it is corrected by the same statement.
        sqlx::query(
            "UPDATE capability SET kind = 'system'
              WHERE tenant_id = $1 AND code IN ('project_management', 'kanban_coach', 'kata')",
        )
        .bind(tenant_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        results.push((
            slug,
            ImportCounts {
                created: card_types.created
                    + entries.created
                    + capability_counts.created
                    + relation_counts.created,
                updated: card_types.updated
                    + entries.updated
                    + capability_counts.updated
                    + relation_counts.updated,
                skipped: card_types.skipped
                    + entries.skipped
                    + capability_counts.skipped
                    + relation_counts.skipped,
            },
        ));
    }
    Ok(results)
}
