//! Gate property (b): a claim never succeeds twice without release or expiry.
//!
//! Thirty-two tasks race for one card. Exactly one is granted; the loser's `UPDATE` matched
//! zero rows because the winner's commit moved `version`. Then sequential renewals must
//! strictly increase `claim_fence`, and a completion presenting a stale fence writes nothing.
//!
//! `cargo test -p conveyor-store --test claim_race -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{TenantId, Version};
use conveyor_store::begin_scoped;
use conveyor_store::card::{claim_card_tx, get_card_tx, release_card_tx, renew_card_tx};
use tokio::task::JoinSet;
use uuid::Uuid;

#[tokio::test]
async fn thirty_two_racers_get_exactly_one_grant_and_the_fence_is_monotonic() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "claim").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    // ── 32 concurrent claims, all from version 1 ────────────────────────────────────────
    let mut racers = JoinSet::new();
    for _ in 0..32 {
        let pool = db.runtime.clone();
        let card = seed.card;
        let who = seed.user;
        racers.spawn(async move {
            let mut tx = begin_scoped(&pool, tenant).await.expect("scope the claim");
            let granted = claim_card_tx(&mut tx, tenant, card, who, 900, Version::INITIAL)
                .await
                .expect("the claim statement runs");
            tx.commit().await.expect("commit the claim");
            granted
        });
    }
    let mut wins = Vec::new();
    while let Some(joined) = racers.join_next().await {
        if let Some(granted) = joined.expect("a racer must not panic") {
            wins.push(granted);
        }
    }
    println!("32 racers produced {} grant(s): {wins:?}", wins.len());
    assert_eq!(wins.len(), 1, "exactly one claim may be granted");
    assert_eq!(wins[0].claim_fence, 1, "the first claim is fence 1");
    assert_eq!(wins[0].version.get(), 2, "one version bump per grant");

    // ── sequential renewals by the same assignee: the fence strictly increases ──────────
    let mut fences = vec![wins[0].claim_fence];
    let mut version = wins[0].version;
    for round in 0..3 {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let granted = claim_card_tx(&mut tx, tenant, seed.card, seed.user, 900, version)
            .await
            .expect("renew")
            .unwrap_or_else(|| panic!("renewal {round} must be granted to the current holder"));
        tx.commit().await.expect("commit the renewal");
        fences.push(granted.claim_fence);
        version = granted.version;
    }
    println!(
        "fences after renewals: {fences:?}, version {}",
        version.get()
    );
    assert!(
        fences.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "claim_fence must strictly increase on every grant: {fences:?}"
    );
    assert_eq!(fences, vec![1, 2, 3, 4]);
    assert_eq!(version.get(), 5);

    // ── a stale-fence completion writes nothing ─────────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let stale = release_card_tx(&mut tx, tenant, seed.card, 1)
        .await
        .expect("the stale release statement runs");
    tx.commit().await.expect("commit the stale release");
    assert!(!stale, "a release presenting fence 1 must write nothing");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let card = get_card_tx(&mut tx, tenant, seed.card)
        .await
        .expect("read the card")
        .expect("the card exists");
    tx.rollback().await.expect("roll back the read");
    println!(
        "after the stale release: fence={} lease_token={:?} assignee={:?}",
        card.claim_fence, card.lease_token, card.assignee_id
    );
    assert_eq!(card.claim_fence, 4, "the stale holder changed nothing");
    assert!(card.lease_token.is_some(), "the live lease still stands");
    assert_eq!(card.assignee_id, Some(seed.user));

    // ── a stale version cannot claim, and the live fence can release ────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let stale_version = claim_card_tx(&mut tx, tenant, seed.card, seed.user, 900, Version::INITIAL)
        .await
        .expect("the stale-version claim statement runs");
    tx.rollback().await.expect("roll back");
    assert_eq!(stale_version, None, "version 1 is stale after four grants");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let released = release_card_tx(&mut tx, tenant, seed.card, 4)
        .await
        .expect("the live release statement runs");
    tx.commit().await.expect("commit the live release");
    assert!(released, "the current fence releases the lease");

    // ── the next claim continues the fence; it is never reset ───────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let card = get_card_tx(&mut tx, tenant, seed.card)
        .await
        .expect("read")
        .expect("exists");
    let granted = claim_card_tx(
        &mut tx,
        tenant,
        seed.card,
        seed.other_user,
        900,
        card.version,
    )
    .await
    .expect("claim after release")
    .expect("a released card is claimable");
    tx.commit().await.expect("commit");
    println!(
        "claim after release: fence={} version={}",
        granted.claim_fence,
        granted.version.get()
    );
    assert_eq!(granted.claim_fence, 5, "the fence survives the release");
}

/// Exclusivity when every racer is a *distinct* `app_user`: the same-assignee arm can grant
/// none of them, and once one wins the live lease must refuse every other user even at the
/// card's current version.
///
/// Thirty-two users race one card at its current version. Exactly one is granted; the losers
/// are `Ok(None)`, the store's refusal (a live lease is the action layer's `409`, a stale
/// version its `412`). The winner then re-claims through the same-assignee arm with the
/// version its grant returned and renews with the fence, and a different user presenting that
/// *current* version is refused — the version predicate cannot explain that refusal, only the
/// live lease can.
#[tokio::test]
async fn thirty_two_users_race_one_card_and_exactly_one_wins() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "claim-users").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    // ── 32 distinct app_users; the fixture's own two users stay out of the race ───────────
    let mut users = Vec::with_capacity(32);
    for racer in 0..32 {
        let user: Uuid =
            sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ($1) RETURNING id")
                .bind(format!("racer-{racer}"))
                .fetch_one(&db.super_pool)
                .await
                .expect("seed a racing app_user");
        users.push(user);
    }

    // ── 32 concurrent claims, one per distinct user, all from the card's version 1 ───────
    let mut racers = JoinSet::new();
    for user in users.iter().copied() {
        let pool = db.runtime.clone();
        let card = seed.card;
        racers.spawn(async move {
            let mut tx = begin_scoped(&pool, tenant).await.expect("scope the claim");
            let granted = claim_card_tx(&mut tx, tenant, card, user, 900, Version::INITIAL)
                .await
                .expect("the claim statement runs");
            tx.commit().await.expect("commit the claim");
            (user, granted)
        });
    }
    let mut winners = Vec::new();
    let mut refusals = 0usize;
    while let Some(joined) = racers.join_next().await {
        let (user, granted) = joined.expect("a racer must not panic");
        match granted {
            Some(granted) => winners.push((user, granted)),
            None => refusals += 1,
        }
    }
    println!(
        "32 distinct users produced {} grant(s) and {refusals} refusal(s)",
        winners.len()
    );
    assert_eq!(
        winners.len(),
        1,
        "exactly one of 32 distinct users may claim the card"
    );
    assert_eq!(
        refusals, 31,
        "the other 31 distinct users are refused with `Ok(None)`"
    );
    let (winner, granted) = winners[0];
    assert_eq!(granted.claim_fence, 1, "the first claim is fence 1");
    assert_eq!(granted.version.get(), 2, "one version bump per grant");

    // ── the holder renews through the same-assignee arm with its granted version ────────
    let mut tx = begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope the renewal");
    let renewed = claim_card_tx(&mut tx, tenant, seed.card, winner, 900, granted.version)
        .await
        .expect("the renewal statement runs")
        .expect("the holder renews with the version its grant returned");
    tx.commit().await.expect("commit the renewal");
    assert_eq!(renewed.claim_fence, 2, "a renewal continues the fence");
    assert_eq!(renewed.version.get(), 3, "a renewal bumps the version");

    // ── the fence itself is a renew credential: the same claim held longer ──────────────
    let mut tx = begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope the fence renewal");
    let extended = renew_card_tx(&mut tx, tenant, seed.card, renewed.claim_fence, 900)
        .await
        .expect("the fence-renew statement runs")
        .expect("the holder renews with the fence its grant returned");
    tx.commit().await.expect("commit the fence renewal");
    assert_eq!(
        extended.claim_fence, renewed.claim_fence,
        "a renew is the same claim held longer"
    );
    assert_eq!(extended.version.get(), 4, "a renew still bumps the version");

    // ── a different user at that current version loses to the live lease alone ──────────
    let stranger = users
        .iter()
        .copied()
        .find(|user| *user != winner)
        .expect("a non-holder among the racers");
    let mut tx = begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope the stranger's claim");
    let refused = claim_card_tx(&mut tx, tenant, seed.card, stranger, 900, extended.version)
        .await
        .expect("the stranger's claim statement runs");
    tx.rollback().await.expect("roll back the refused claim");
    assert_eq!(
        refused, None,
        "a distinct user cannot claim a card under a live lease, even at its current version"
    );
}
