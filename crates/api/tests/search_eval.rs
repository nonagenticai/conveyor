//! Search relevance gate (S07 §5.6) — the shipped `search.query` engine measured against a
//! naive `ILIKE` floor on a labelled 40-query eval set.
//!
//! `search.query` ranks governed assets with Postgres full-text search plus `pg_trgm` word
//! similarity. "It returns rows" is not the property that matters: the property that matters
//! is that the ranking is **meaningfully better than a substring match**, because otherwise
//! the weights, the `tsv` column and the lowered `word_similarity_threshold` are all dead
//! weight and a reviewer cannot tell a regression from a reshuffle. This file is that gate.
//!
//! Two tests:
//!
//! - [`the_eval_data_is_structurally_sound`] — a plain `#[test]` that reads
//!   `tests/eval/corpus.json` and `tests/eval/search_queries.json` and asserts the shape the
//!   eval depends on: 128 assets with unique keys, 40 queries with unique ids and
//!   case-insensitively unique texts, the exact intent mix (`exact_key` 10, `title` 10,
//!   `body_phrase` 8, `paraphrase` 6, `typo` 6), every expected key present in the corpus,
//!   1–2 expected keys per query, and every typo a single whitespace token. Editing the data
//!   into something the eval cannot measure fails here, loudly, before any database starts.
//!
//! - [`full_text_beats_the_ilike_floor`] — `#[ignore]`d because it starts its own
//!   `postgres:18` container: it seeds the 128 assets into one tenant, asks the real served
//!   action for the top 5 on every query, and compares it against a raw-SQL floor that
//!   requires **every** whitespace term to appear as a case-insensitive substring of
//!   `key || ' ' || title || ' ' || body`. It prints a per-intent recall@5 / MRR table and
//!   asserts, fail-closed: B recall@5 ≥ 0.75, B MRR ≥ 0.60, B recall@5 ≥ A recall@5 + 0.20,
//!   the floor is not already perfect on the `typo` and `paraphrase` intents (guard a), no
//!   intent is below 0.60 on the engine (guard b), and the engine never loses to the floor on
//!   any intent.
//!
//! The weights are committed constants in `crates/store/src/search.rs`; this file measures
//! them and never tunes them. A miss reports the numbers and the failing ids — the decision
//! belongs to the slice owner.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use uuid::Uuid;

/// The corpus the engine is measured against (128 labelled assets).
const CORPUS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/eval/corpus.json");

/// The labelled eval set (40 queries).
const QUERIES_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/eval/search_queries.json"
);

/// The five intents, in the order the table and the guards read them.
const INTENTS: [&str; 5] = ["exact_key", "title", "body_phrase", "paraphrase", "typo"];

/// The corpus size the labels were written against.
const CORPUS_SIZE: usize = 128;

/// The eval-set size the labels were written against.
const QUERY_COUNT: usize = 40;

/// The intent mix the design names: `exact_key` 10, `title` 10, `body_phrase` 8,
/// `paraphrase` 6, `typo` 6.
const INTENT_MIX: [(&str, usize); 5] = [
    ("exact_key", 10),
    ("title", 10),
    ("body_phrase", 8),
    ("paraphrase", 6),
    ("typo", 6),
];

/// Arm A, the floor: every whitespace-separated query term must appear as a
/// case-insensitive substring of the concatenated key, title and body. `NOT EXISTS` over
/// `unnest` is "no term is missing"; `ORDER BY key` makes the five rows reproducible.
const ILIKE_FLOOR: &str = "SELECT key FROM governed_asset \
     WHERE tenant_id = $1 \
       AND NOT EXISTS ( \
           SELECT 1 FROM unnest($2::text[]) AS t \
            WHERE lower(coalesce(key, '') || ' ' || coalesce(title, '') || ' ' \
                        || coalesce(body, '')) \
                  NOT LIKE '%' || lower(t) || '%') \
     ORDER BY key \
     LIMIT 5";

/// One asset, with the successor pointer the schema demands for a `superseded` row.
const INSERT_ASSET: &str = "INSERT INTO governed_asset \
        (tenant_id, kind, status, key, title, body, superseded_by_id) \
     VALUES ($1, $2, $3, $4, $5, $6, $7)";

/// Read one eval JSON file from disk, with the path in every failure message.
fn load(path: &str) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read the eval data at {path}: {error}"));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("the eval data at {path} is not JSON: {error}"))
}

/// The corpus's assets array.
fn assets_of(corpus: &Value) -> &Vec<Value> {
    corpus["assets"]
        .as_array()
        .unwrap_or_else(|| panic!("corpus.json has no `assets` array"))
}

/// The query set's queries array.
fn queries_of(queries: &Value) -> &Vec<Value> {
    queries["queries"]
        .as_array()
        .unwrap_or_else(|| panic!("search_queries.json has no `queries` array"))
}

/// The string member `field` of one labelled record.
fn text_of(row: &Value, field: &str) -> String {
    row[field]
        .as_str()
        .unwrap_or_else(|| panic!("a labelled record has no string `{field}`: {row}"))
        .to_owned()
}

/// `|expected ∩ top5| / |expected|`. An empty expectation cannot be measured and scores 0.
fn recall_at_five(expected: &[String], top: &[String]) -> f64 {
    if expected.is_empty() {
        return 0.0;
    }
    let found = expected
        .iter()
        .filter(|wanted| top.iter().any(|key| key == *wanted))
        .count();
    found as f64 / expected.len() as f64
}

/// `1 / rank` of the first expected key in `top` (1-based); 0 when none is present.
fn reciprocal_rank(expected: &[String], top: &[String]) -> f64 {
    expected
        .iter()
        .filter_map(|wanted| top.iter().position(|key| key == wanted))
        .min()
        .map_or(0.0, |position| 1.0 / (position as f64 + 1.0))
}

// ── test 1: the data guard ──────────────────────────────────────────────────────────────

#[test]
fn the_eval_data_is_structurally_sound() {
    let corpus = load(CORPUS_PATH);
    let queries = load(QUERIES_PATH);

    // 128 assets, each with a string key, and no two keys alike.
    let assets = assets_of(&corpus);
    assert_eq!(
        assets.len(),
        CORPUS_SIZE,
        "the corpus must hold exactly {CORPUS_SIZE} labelled assets"
    );
    let mut keys: Vec<String> = Vec::with_capacity(assets.len());
    for asset in assets {
        keys.push(text_of(asset, "key"));
    }
    let corpus_keys: HashSet<&str> = keys.iter().map(String::as_str).collect();
    let mut duplicates: Vec<&str> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::with_capacity(keys.len());
    for key in &keys {
        if !seen.insert(key.as_str()) {
            duplicates.push(key.as_str());
        }
    }
    assert!(
        duplicates.is_empty(),
        "asset keys must be unique so a labelled expectation names exactly one row; duplicates: {duplicates:?}"
    );
    assert_eq!(corpus_keys.len(), CORPUS_SIZE);

    // 40 queries, unique ids, unique texts (case-insensitive).
    let rows = queries_of(&queries);
    assert_eq!(
        rows.len(),
        QUERY_COUNT,
        "the eval set must hold exactly {QUERY_COUNT} queries"
    );
    let mut ids: HashSet<String> = HashSet::with_capacity(rows.len());
    let mut texts: HashSet<String> = HashSet::with_capacity(rows.len());
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for query in rows {
        let id = text_of(query, "id");
        assert!(ids.insert(id.clone()), "duplicate query id {id}");

        let text = text_of(query, "text");
        assert!(
            texts.insert(text.to_lowercase()),
            "duplicate query text (case-insensitive) on {id}: {text:?}"
        );

        let intent = text_of(query, "intent");
        *counts.entry(intent_key(&intent)).or_insert(0) += 1;

        let expected: Vec<String> = query["expected"]
            .as_array()
            .unwrap_or_else(|| panic!("{id} has no `expected` array: {query}"))
            .iter()
            .map(|key| {
                key.as_str()
                    .unwrap_or_else(|| panic!("{id} has a non-string expected key: {key}"))
                    .to_owned()
            })
            .collect();
        assert!(
            (1..=2).contains(&expected.len()),
            "{id} must expect 1-2 keys, it expects {}",
            expected.len()
        );
        for key in &expected {
            assert!(
                corpus_keys.contains(key.as_str()),
                "{id} expects {key}, which is not in the corpus"
            );
        }

        if intent == "typo" {
            assert_eq!(
                text.split_whitespace().count(),
                1,
                "{id} is a typo query, so its text must be a single whitespace token: {text:?}"
            );
        }
    }

    // Exactly the five intents the design names, in exactly the labelled proportions.
    assert_eq!(
        counts.len(),
        INTENT_MIX.len(),
        "the eval must use exactly the five named intents; saw {counts:?}"
    );
    for (intent, expected) in INTENT_MIX {
        assert_eq!(
            counts.get(intent).copied().unwrap_or(0),
            expected,
            "intent {intent} must have {expected} queries; counts: {counts:?}"
        );
    }
}

// ── test 2: the relevance gate ──────────────────────────────────────────────────────────

/// One query's measured outcome under both arms.
struct Outcome {
    id: String,
    text: String,
    intent: &'static str,
    expected: Vec<String>,
    b_keys: Vec<String>,
    a_recall: f64,
    b_recall: f64,
    a_mrr: f64,
    b_mrr: f64,
}

/// Sums for one intent (or for the whole set).
#[derive(Default)]
struct Tally {
    n: usize,
    a_recall: f64,
    b_recall: f64,
    a_mrr: f64,
    b_mrr: f64,
}

impl Tally {
    fn add(&mut self, outcome: &Outcome) {
        self.n += 1;
        self.a_recall += outcome.a_recall;
        self.b_recall += outcome.b_recall;
        self.a_mrr += outcome.a_mrr;
        self.b_mrr += outcome.b_mrr;
    }

    fn mean_a_recall(&self) -> f64 {
        self.a_recall / self.n as f64
    }

    fn mean_b_recall(&self) -> f64 {
        self.b_recall / self.n as f64
    }

    fn mean_a_mrr(&self) -> f64 {
        self.a_mrr / self.n as f64
    }

    fn mean_b_mrr(&self) -> f64 {
        self.b_mrr / self.n as f64
    }
}

/// One intent, as a `'static` name the tallies can key on.
fn intent_key(intent: &str) -> &'static str {
    INTENTS
        .into_iter()
        .find(|known| *known == intent)
        .unwrap_or_else(|| panic!("{intent:?} is not one of the five named intents"))
}

/// Seed the whole corpus into `tenant_id` through the superuser pool, so RLS cannot hide a
/// row the eval would then silently score as absent.
async fn insert_corpus(pg: &support::Pg, tenant_id: Uuid, corpus: &Value) {
    let assets = assets_of(corpus);

    for asset in assets
        .iter()
        .filter(|asset| asset["status"] != "superseded")
    {
        insert_asset(pg, tenant_id, asset, None).await;
    }

    // `governed_asset_superseded_pointer` refuses a `superseded` row without a successor,
    // and the eval data names no replacement. The pointer is schema bookkeeping —
    // `search.query` never reads it — so every superseded row points at an asset already
    // inserted and the status the eval ranks stays exactly the labelled one.
    let successor: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM governed_asset WHERE tenant_id = $1 ORDER BY key LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pg.super_pool)
    .await
    .expect("read a successor row");
    let Some(successor) = successor else {
        panic!("the corpus holds only superseded assets, so there is no successor to point at");
    };
    for asset in assets
        .iter()
        .filter(|asset| asset["status"] == "superseded")
    {
        insert_asset(pg, tenant_id, asset, Some(successor)).await;
    }
}

/// Insert one labelled asset verbatim.
async fn insert_asset(
    pg: &support::Pg,
    tenant_id: Uuid,
    asset: &Value,
    superseded_by: Option<Uuid>,
) {
    let key = text_of(asset, "key");
    sqlx::query(INSERT_ASSET)
        .bind(tenant_id)
        .bind(text_of(asset, "kind"))
        .bind(text_of(asset, "status"))
        .bind(&key)
        .bind(text_of(asset, "title"))
        .bind(text_of(asset, "body"))
        .bind(superseded_by)
        .execute(&pg.super_pool)
        .await
        .unwrap_or_else(|error| panic!("insert {key}: {error}"));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts its own postgres:18 container and seeds 128 rows; run with --ignored"]
async fn full_text_beats_the_ilike_floor() {
    let corpus = load(CORPUS_PATH);
    let queries = load(QUERIES_PATH);

    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "eval", "eval", "Eval").await;
    insert_corpus(&pg, seeded.tenant_id, &corpus).await;
    sqlx::query("ANALYZE governed_asset")
        .execute(&pg.super_pool)
        .await
        .expect("ANALYZE governed_asset");

    let asset_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM governed_asset WHERE tenant_id = $1")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count the seeded assets");
    assert_eq!(
        asset_count, CORPUS_SIZE as i64,
        "every labelled asset must be seeded before the eval can measure anything"
    );

    let running = support::Running::start(&pg).await;

    // Arm A's reads run in one tenant-scoped transaction on the runtime role, so RLS applies
    // exactly as it does to the action.
    let mut floor_tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped floor transaction");

    let mut outcomes: Vec<Outcome> = Vec::with_capacity(QUERY_COUNT);
    for query in queries_of(&queries) {
        let id = text_of(query, "id");
        let text = text_of(query, "text");
        let intent = intent_key(&text_of(query, "intent"));
        let expected: Vec<String> = query["expected"]
            .as_array()
            .unwrap_or_else(|| panic!("{id} has no `expected` array"))
            .iter()
            .map(|key| {
                key.as_str()
                    .unwrap_or_else(|| panic!("{id} has a non-string expected key: {key}"))
                    .to_owned()
            })
            .collect();

        // Arm B — the shipped engine, through the real action surface.
        let (status, body) = running
            .invoke(
                "search.query",
                "dev:eval",
                json!({"q": text, "types": ["governed_asset"], "limit": 5}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "{id}: search.query refused {text:?}: {body}");
        let items = body["output"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("{id}: the response has no output.items: {body}"));
        let b_keys: Vec<String> = items
            .iter()
            .map(|hit| {
                hit["key"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{id}: a governed-asset hit has no key: {hit}"))
                    .to_owned()
            })
            .collect();

        // Arm A — the naive floor, every term a case-insensitive substring.
        let terms: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
        let a_keys: Vec<String> = sqlx::query_scalar(ILIKE_FLOOR)
            .bind(seeded.tenant_id)
            .bind(&terms)
            .fetch_all(&mut *floor_tx)
            .await
            .unwrap_or_else(|error| panic!("{id}: the ILIKE floor query failed: {error}"));

        outcomes.push(Outcome {
            id,
            text,
            intent,
            a_recall: recall_at_five(&expected, &a_keys),
            b_recall: recall_at_five(&expected, &b_keys),
            a_mrr: reciprocal_rank(&expected, &a_keys),
            b_mrr: reciprocal_rank(&expected, &b_keys),
            b_keys,
            expected,
        });
    }
    floor_tx.commit().await.expect("commit the floor reads");
    running.shutdown().await;

    // ── the table ───────────────────────────────────────────────────────────────────────
    let mut per_intent: HashMap<&'static str, Tally> = INTENTS
        .into_iter()
        .map(|intent| (intent, Tally::default()))
        .collect();
    let mut overall = Tally::default();
    for outcome in &outcomes {
        per_intent
            .get_mut(outcome.intent)
            .expect("every outcome's intent is one of the five")
            .add(outcome);
        overall.add(outcome);
    }

    println!("intent       |   n | A recall@5 | B recall@5 |   A MRR |   B MRR");
    for intent in INTENTS {
        let tally = &per_intent[intent];
        assert!(
            tally.n > 0,
            "intent {intent} has no queries, so the gate cannot measure it"
        );
        println!(
            "{:<12} | {:>3} | {:>10.3} | {:>10.3} | {:>7.3} | {:>7.3}",
            intent,
            tally.n,
            tally.mean_a_recall(),
            tally.mean_b_recall(),
            tally.mean_a_mrr(),
            tally.mean_b_mrr()
        );
    }
    println!(
        "{:<12} | {:>3} | {:>10.3} | {:>10.3} | {:>7.3} | {:>7.3}",
        "overall",
        overall.n,
        overall.mean_a_recall(),
        overall.mean_b_recall(),
        overall.mean_a_mrr(),
        overall.mean_b_mrr()
    );
    println!(
        "{:<12} | {:>3} | {:>10} | {:>10.3} | {:>7} | {:>7}",
        "B-A gate",
        "-",
        "-",
        overall.mean_b_recall() - overall.mean_a_recall(),
        "-",
        "-"
    );
    println!("(the B-A gate requires B recall@5 - A recall@5 >= 0.20)");

    println!();
    println!("queries where the shipped engine scored below recall@5 = 1.000:");
    let mut missed = 0usize;
    for outcome in outcomes.iter().filter(|outcome| outcome.b_recall < 1.0) {
        missed += 1;
        println!(
            "  {} | {} | expected {:?} | recall@5 {:.3} | returned top5 {:?}",
            outcome.id, outcome.text, outcome.expected, outcome.b_recall, outcome.b_keys
        );
    }
    if missed == 0 {
        println!("  (none)");
    }

    // ── the gate ────────────────────────────────────────────────────────────────────────
    let b_ids: Vec<&str> = outcomes
        .iter()
        .filter(|outcome| outcome.b_recall < 1.0)
        .map(|outcome| outcome.id.as_str())
        .collect();

    assert!(
        overall.mean_b_recall() >= 0.75,
        "B recall@5 {:.3} < 0.75; queries below 1.000: {b_ids:?}",
        overall.mean_b_recall()
    );
    assert!(
        overall.mean_b_mrr() >= 0.60,
        "B MRR {:.3} < 0.60; queries below recall@5 1.000: {b_ids:?}",
        overall.mean_b_mrr()
    );
    assert!(
        overall.mean_b_recall() >= overall.mean_a_recall() + 0.20,
        "B recall@5 {:.3} is not 0.20 above the ILIKE floor {:.3} (gap {:.3}); queries below 1.000: {b_ids:?}",
        overall.mean_b_recall(),
        overall.mean_a_recall(),
        overall.mean_b_recall() - overall.mean_a_recall()
    );

    // Guard a: a floor that already reaches everything on the intents the engine exists for
    // would make the comparison vacuous.
    let typo = &per_intent["typo"];
    let paraphrase = &per_intent["paraphrase"];
    assert!(
        typo.mean_a_recall() < 1.0,
        "guard a: the ILIKE floor already scores 1.000 on `typo` — the set does not measure what the design claims"
    );
    assert!(
        paraphrase.mean_a_recall() < 1.0,
        "guard a: the ILIKE floor already scores 1.000 on `paraphrase` — the set does not measure what the design claims"
    );

    // Guard b and the per-intent comparison.
    for intent in INTENTS {
        let tally = &per_intent[intent];
        assert!(
            tally.mean_b_recall() >= 0.60,
            "guard b: B recall@5 on `{intent}` is {:.3} < 0.60 (n={})",
            tally.mean_b_recall(),
            tally.n
        );
        assert!(
            tally.mean_b_recall() >= tally.mean_a_recall(),
            "B recall@5 on `{intent}` is {:.3}, below the ILIKE floor's {:.3} (n={})",
            tally.mean_b_recall(),
            tally.mean_a_recall(),
            tally.n
        );
    }
}
