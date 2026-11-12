//! The board spine: programme, project, board, list, card type and card.
//!
//! **The list is the state** (CONTRACT §4). There is no `card.state` field and no accessor
//! that names one: the lifecycle is `list_id` + `archived_at`. A move is legal only when the
//! project-type matrix says so; `may_move` is the one function that answers that, and every
//! caller (API, action, process runner) uses it.
//!
//! `archived_at` and `list.kind='archive'` are two depths of one idea (S3-D2): the kind is a
//! positionable lane, the stamp is exclusion from every hot index. The invariant, proptested
//! below, is `archived_at.is_some() ⇒ the card's list is an archive list`. A stamped card is
//! not draggable — it is restored (stamp cleared) and then moved — so the invariant cannot be
//! broken by a move racing a sweep.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Timestamp;
use crate::catalogue::{CardTypeRules, Code};
use crate::ids::Version;

/// The five card lanes. `kind` is the only lifecycle vocabulary in the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListKind {
    /// New work, not yet accepted.
    Intake,
    /// Being worked.
    Active,
    /// Blocked on something outside the board.
    Waiting,
    /// Finished work, still visible.
    Done,
    /// The archive lane: a card can sit here before it is stamped.
    Archive,
}

impl ListKind {
    /// Every kind, in lifecycle order.
    pub const ALL: [ListKind; 5] = [
        ListKind::Intake,
        ListKind::Active,
        ListKind::Waiting,
        ListKind::Done,
        ListKind::Archive,
    ];

    /// The stored `list.kind` / matrix spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ListKind::Intake => "intake",
            ListKind::Active => "active",
            ListKind::Waiting => "waiting",
            ListKind::Done => "done",
            ListKind::Archive => "archive",
        }
    }
}

impl fmt::Display for ListKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why a string is not a list kind.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown list kind {0:?}: the five kinds are intake, active, waiting, done, archive")]
pub struct ListKindError(pub String);

impl FromStr for ListKind {
    type Err = ListKindError;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        ListKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == raw)
            .ok_or_else(|| ListKindError(raw.to_owned()))
    }
}

/// A portfolio container.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Programme {
    /// The row id.
    pub id: uuid::Uuid,
    /// Display name; unique per tenant.
    pub name: String,
    /// Free text status (`active` by default); no vocabulary is fixed for it.
    pub status: String,
    /// The accountable user, when one is named.
    pub owner_user_id: Option<uuid::Uuid>,
    /// The optimistic-concurrency version.
    pub version: Version,
}

/// The one project entity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// The row id.
    pub id: uuid::Uuid,
    /// The short key; unique per tenant.
    pub key: String,
    /// Display name.
    pub name: String,
    /// Free text, empty by default.
    pub description: String,
    /// `draft | active | paused | closed | archived`.
    pub status: String,
    /// The programme this project rolls up to.
    pub programme_id: Option<uuid::Uuid>,
    /// The project-type code; the matrix row is `catalogue_entry(kind='project_type').code`.
    pub project_type_code: Option<String>,
    /// The current delivery phase.
    pub phase_code: Option<String>,
    /// The accountable user.
    pub owner_user_id: Option<uuid::Uuid>,
    /// The source repository, when it has one.
    pub repo_url: Option<String>,
    /// Kind-specific fields, read whole with the row.
    pub meta: Value,
    /// The optimistic-concurrency version.
    pub version: Version,
}

/// One board. The FK, not a 1:1 assumption, is the contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Board {
    /// The row id.
    pub id: uuid::Uuid,
    /// The owning project.
    pub project_id: uuid::Uuid,
    /// Display name; unique per project.
    pub name: String,
    /// The optimistic-concurrency version.
    pub version: Version,
}

/// A card lane. `kind` is the card's state; a lane is a place, not a status code.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct List {
    /// The row id.
    pub id: uuid::Uuid,
    /// The owning board.
    pub board_id: uuid::Uuid,
    /// Display name.
    pub name: String,
    /// The lane's lifecycle meaning.
    pub kind: ListKind,
    /// A soft cap; the instance of the `wip_limit` catalogue template.
    pub wip_limit: Option<i32>,
    /// The optimistic-concurrency version.
    pub version: Version,
}

/// A card's type; the join target of the project-type matrix.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CardType {
    /// The row id.
    pub id: uuid::Uuid,
    /// The stable code the matrix is keyed by.
    pub code: Code,
    /// The display label.
    pub label: String,
    /// Type-specific fields, read whole with the row.
    pub definition: Value,
    /// Retired types stay readable but cannot be chosen.
    pub is_active: bool,
    /// The optimistic-concurrency version.
    pub version: Version,
}

/// A work item.
///
/// There is no lifecycle column and no field or accessor that names one. The claim fields
/// are a lease carried on the card row itself (CONTRACT §4): `lease_expires_at` and
/// `lease_token` are set and cleared together, and `claim_fence` is monotonic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Card {
    /// The row id.
    pub id: uuid::Uuid,
    /// The lane the card sits in — its state.
    pub list_id: uuid::Uuid,
    /// The card's type.
    pub card_type_id: uuid::Uuid,
    /// Denormalised from `list.board_id`; every move keeps them equal.
    pub board_id: uuid::Uuid,
    /// The exclusion stamp. `Some` means the card is out of every hot index.
    pub archived_at: Option<Timestamp>,
    /// The current claimer, when leased.
    pub assignee_id: Option<uuid::Uuid>,
    /// When the lease expires; set together with `lease_token`.
    pub lease_expires_at: Option<Timestamp>,
    /// The claim's fence token; set together with `lease_expires_at`.
    pub lease_token: Option<uuid::Uuid>,
    /// Monotonic claim counter, never reset while the card exists.
    pub claim_fence: i64,
    /// The lane the card came from, for undo and the activity feed.
    pub prev_list_id: Option<uuid::Uuid>,
    /// When the card last changed lane.
    pub list_changed_at: Timestamp,
    /// The optimistic-concurrency version.
    pub version: Version,
}

/// A legal move: the caller may now write `list_id`, `prev_list_id`, `list_changed_at` and
/// `version` in one statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Move {
    /// The lane being left.
    pub from: ListKind,
    /// The lane being entered.
    pub to: ListKind,
}

/// Why a move is refused. Every variant is a denial, never a default allow (S3-D3).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MoveError {
    /// The rules were resolved for a different card type.
    #[error("the card's type has no rules in this project type's matrix")]
    UnknownCardType,
    /// A stamped card is not draggable: restore it first.
    #[error("an archived card must be restored before it can move")]
    Archived,
    /// The matrix has no row for the lane being left.
    #[error("the matrix has no entry for lists of kind {0}")]
    NoEntry(ListKind),
    /// The row exists and does not list the destination.
    #[error("a card cannot move from {from} to {to}")]
    NotAllowed {
        /// The lane being left.
        from: ListKind,
        /// The lane being entered.
        to: ListKind,
    },
}

/// Why an archive or restore is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ArchiveError {
    /// The other list is not an archive lane.
    #[error("the archive lane is required")]
    NotArchiveList,
    /// The stamp is already set.
    #[error("the card is already archived")]
    AlreadyArchived,
    /// There is no stamp to clear.
    #[error("the card is not archived")]
    NotArchived,
    /// Restoring to the requested lane is not a legal move for this card type.
    #[error("restore is not a legal move: {0}")]
    Move(MoveError),
}

/// The one legality function for moving a card between lanes.
///
/// Absent matrix row, absent card type, or a rules set resolved for another card type all
/// deny. There is no wildcard and no "unknown means allow".
pub fn may_move(
    card: &Card,
    from: ListKind,
    to: ListKind,
    rules: &CardTypeRules,
) -> Result<Move, MoveError> {
    if card.archived_at.is_some() {
        return Err(MoveError::Archived);
    }
    if rules.card_type_id() != card.card_type_id {
        return Err(MoveError::UnknownCardType);
    }
    let allowed = rules.allowed_from(from).ok_or(MoveError::NoEntry(from))?;
    if !allowed.contains(&to) {
        return Err(MoveError::NotAllowed { from, to });
    }
    Ok(Move { from, to })
}

impl Card {
    /// Stamp a card that already sits in (or is being placed into) the archive lane.
    ///
    /// The stamp is exclusion, not a state: the card keeps its `list_id` and stays reachable
    /// from the restore screen only. `archive` refuses a non-archive lane so the invariant
    /// `archived_at ⇒ list.kind='archive'` holds by construction.
    pub fn archive(mut self, list: &List, at: Timestamp) -> Result<Card, ArchiveError> {
        if list.kind != ListKind::Archive {
            return Err(ArchiveError::NotArchiveList);
        }
        if self.archived_at.is_some() {
            return Err(ArchiveError::AlreadyArchived);
        }
        self.prev_list_id = Some(self.list_id);
        self.list_id = list.id;
        self.board_id = list.board_id;
        self.list_changed_at = at;
        self.archived_at = Some(at);
        self.version = self.version.next();
        Ok(self)
    }

    /// Clear the stamp and place the card back on a live lane.
    ///
    /// `from` is the lane the card currently occupies and must be the archive lane: a restore
    /// that starts anywhere else is refused, which is what keeps the stamp from orphaning. The
    /// destination is a second `may_move` question and is denied when the matrix has no
    /// `archive → to` row.
    pub fn restore(
        mut self,
        from: &List,
        to: &List,
        rules: &CardTypeRules,
        at: Timestamp,
    ) -> Result<Card, ArchiveError> {
        if from.kind != ListKind::Archive {
            return Err(ArchiveError::NotArchiveList);
        }
        if self.archived_at.is_none() {
            return Err(ArchiveError::NotArchived);
        }
        if rules.card_type_id() != self.card_type_id {
            return Err(ArchiveError::Move(MoveError::UnknownCardType));
        }
        let allowed = rules
            .allowed_from(from.kind)
            .ok_or(ArchiveError::Move(MoveError::NoEntry(from.kind)))?;
        if !allowed.contains(&to.kind) {
            return Err(ArchiveError::Move(MoveError::NotAllowed {
                from: from.kind,
                to: to.kind,
            }));
        }
        self.prev_list_id = Some(self.list_id);
        self.list_id = to.id;
        self.board_id = to.board_id;
        self.archived_at = None;
        self.list_changed_at = at;
        self.version = self.version.next();
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::{CatalogueEntry, CatalogueKind};
    use crate::ids::Id;
    use proptest::prelude::*;
    use serde_json::json;

    fn at(seconds: i64) -> Timestamp {
        chrono::DateTime::from_timestamp(seconds, 0).expect("a valid instant")
    }

    fn list(kind: ListKind) -> List {
        List {
            id: uuid::Uuid::now_v7(),
            board_id: uuid::Uuid::now_v7(),
            name: kind.as_str().to_owned(),
            kind,
            wip_limit: None,
            version: Version::INITIAL,
        }
    }

    fn card_on(list: &List, card_type_id: uuid::Uuid) -> Card {
        Card {
            id: uuid::Uuid::now_v7(),
            list_id: list.id,
            card_type_id,
            board_id: list.board_id,
            archived_at: None,
            assignee_id: None,
            lease_expires_at: None,
            lease_token: None,
            claim_fence: 0,
            prev_list_id: None,
            list_changed_at: at(1_700_000_000),
            version: Version::INITIAL,
        }
    }

    fn card_type() -> CardType {
        CardType {
            id: uuid::Uuid::now_v7(),
            code: Code::parse("feature").expect("code"),
            label: "Feature".to_owned(),
            definition: json!({}),
            is_active: true,
            version: Version::INITIAL,
        }
    }

    /// A matrix that allows `archive -> every kind`, so a restore that starts on the archive
    /// lane always has a legal destination and the test isolates the `from` rule.
    fn permissive_rules(card_type: &CardType) -> CardTypeRules {
        let entry = CatalogueEntry {
            id: uuid::Uuid::now_v7(),
            kind: CatalogueKind::ProjectType,
            code: Code::parse("delivery").expect("code"),
            label: "Delivery".to_owned(),
            meta: json!({
                "matrix": {
                    "feature": {
                        "intake": ["active", "archive"],
                        "active": ["waiting", "done", "archive"],
                        "waiting": ["active", "archive"],
                        "done": ["archive"],
                        "archive": ["intake", "active", "waiting", "done", "archive"]
                    }
                }
            }),
            sort: 0,
            is_active: true,
            version: Version::INITIAL,
        };
        CardTypeRules::from_entry(&entry, card_type).expect("the permissive matrix resolves")
    }

    fn any_kind() -> impl Strategy<Value = ListKind> {
        prop::sample::select(ListKind::ALL.to_vec())
    }

    #[test]
    fn the_worked_example_is_exactly_the_matrix() {
        let ct = card_type();
        let rules = permissive_rules(&ct);
        let live = list(ListKind::Active);
        let card = card_on(&live, ct.id);

        assert_eq!(
            may_move(&card, ListKind::Active, ListKind::Waiting, &rules),
            Ok(Move {
                from: ListKind::Active,
                to: ListKind::Waiting
            })
        );
        assert_eq!(
            may_move(&card, ListKind::Intake, ListKind::Done, &rules),
            Err(MoveError::NotAllowed {
                from: ListKind::Intake,
                to: ListKind::Done
            })
        );
        // A rules set resolved for another card type denies.
        let another = card_type();
        assert_eq!(
            may_move(
                &card,
                ListKind::Active,
                ListKind::Waiting,
                &permissive_rules(&another)
            ),
            Err(MoveError::UnknownCardType)
        );
        // An absent row denies.
        let waiting = list(ListKind::Waiting);
        let waiting_card = card_on(&waiting, ct.id);
        let small = CatalogueEntry {
            id: uuid::Uuid::now_v7(),
            kind: CatalogueKind::ProjectType,
            code: Code::parse("delivery").expect("code"),
            label: "Delivery".to_owned(),
            meta: json!({"matrix": {"feature": {"intake": ["active"]}}}),
            sort: 0,
            is_active: true,
            version: Version::INITIAL,
        };
        let small = CardTypeRules::from_entry(&small, &ct).expect("resolves");
        assert_eq!(
            may_move(&waiting_card, ListKind::Waiting, ListKind::Active, &small),
            Err(MoveError::NoEntry(ListKind::Waiting))
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Gate property (a), first half: a stamped card is always on an archive lane.
        #[test]
        fn archived_implies_the_card_sits_in_an_archive_list(
            attempts in prop::collection::vec(any_kind(), 0..40)
        ) {
            let mut card = {
                let start = list(ListKind::Intake);
                card_on(&start, uuid::Uuid::now_v7())
            };
            let mut current = list(ListKind::Intake);
            card.list_id = current.id;
            card.board_id = current.board_id;

            for (step, kind) in attempts.into_iter().enumerate() {
                let target = list(kind);
                match card.clone().archive(&target, at(1_700_000_000 + step as i64)) {
                    Ok(next) => {
                        prop_assert_eq!(target.kind, ListKind::Archive);
                        prop_assert!(next.archived_at.is_some());
                        prop_assert_eq!(next.list_id, target.id);
                        card = next;
                        current = target;
                    }
                    Err(ArchiveError::NotArchiveList) => {
                        prop_assert_ne!(target.kind, ListKind::Archive);
                    }
                    Err(ArchiveError::AlreadyArchived) => {
                        // The first check in `archive` is the lane kind, so this arm is
                        // reachable only for a second stamp onto an archive lane.
                        prop_assert_eq!(target.kind, ListKind::Archive);
                        prop_assert!(card.archived_at.is_some());
                    }
                    Err(other) => {
                        return Err(TestCaseError::fail(format!(
                            "unexpected archive error: {other}"
                        )));
                    }
                }
                if card.archived_at.is_some() {
                    prop_assert_eq!(current.kind, ListKind::Archive);
                }
            }
        }

        /// Gate property (a), second half: `restore` off a non-archive lane is an error.
        #[test]
        fn restore_off_a_non_archive_list_is_an_error(
            from_kind in any_kind(),
            to_kind in any_kind(),
        ) {
            let ct = card_type();
            let rules = permissive_rules(&ct);
            let archive = list(ListKind::Archive);
            let archived = card_on(&archive, ct.id)
                .archive(&archive, at(1_700_000_000))
                .expect("archiving on an archive lane succeeds");

            let from = list(from_kind);
            let to = list(to_kind);
            let outcome = archived.restore(&from, &to, &rules, at(1_700_000_100));
            if from_kind != ListKind::Archive {
                prop_assert_eq!(outcome, Err(ArchiveError::NotArchiveList));
            } else {
                let restored = outcome.map_err(|e| TestCaseError::fail(format!("{e}")))?;
                prop_assert!(restored.archived_at.is_none());
                prop_assert_eq!(restored.list_id, to.id);
            }
        }
    }

    #[test]
    fn archive_requires_an_archive_lane_and_restore_requires_a_stamp() {
        let ct = card_type();
        let rules = permissive_rules(&ct);
        let active = list(ListKind::Active);
        let archive = list(ListKind::Archive);
        let card = card_on(&active, ct.id);

        assert_eq!(
            card.clone().archive(&active, at(1)),
            Err(ArchiveError::NotArchiveList)
        );
        assert_eq!(
            card.clone().restore(&archive, &active, &rules, at(1)),
            Err(ArchiveError::NotArchived)
        );

        let archived = card.archive(&archive, at(1)).expect("archives");
        assert_eq!(archived.archived_at, Some(at(1)));
        assert_eq!(archived.prev_list_id, Some(active.id));
        assert_eq!(
            archived.clone().archive(&archive, at(2)),
            Err(ArchiveError::AlreadyArchived)
        );
        // Moving a stamped card is refused; restore is the only way out.
        assert_eq!(
            may_move(&archived, ListKind::Archive, ListKind::Intake, &rules),
            Err(MoveError::Archived)
        );
        let restored = archived
            .restore(&archive, &active, &rules, at(3))
            .expect("restores");
        assert_eq!(restored.archived_at, None);
        assert_eq!(restored.list_id, active.id);
        assert_eq!(restored.prev_list_id, Some(archive.id));
    }

    #[test]
    fn the_card_field_set_is_pinned_by_a_snapshot() {
        // The card-state proof (S03 §5): a snapshot of the serialised card pins the field
        // set, so a future `state`/`status` field cannot arrive unnoticed.
        let card = Card {
            id: Id::parse("0195f48b-d000-7000-8000-000000000010")
                .expect("id")
                .as_uuid(),
            list_id: Id::parse("0195f48b-d000-7000-8000-000000000011")
                .expect("id")
                .as_uuid(),
            card_type_id: Id::parse("0195f48b-d000-7000-8000-000000000012")
                .expect("id")
                .as_uuid(),
            board_id: Id::parse("0195f48b-d000-7000-8000-000000000013")
                .expect("id")
                .as_uuid(),
            archived_at: None,
            assignee_id: None,
            lease_expires_at: None,
            lease_token: None,
            claim_fence: 3,
            prev_list_id: None,
            list_changed_at: at(1_700_000_000),
            version: Version::new(4).expect("version"),
        };
        let value = serde_json::to_value(&card).expect("card serialises");
        insta::assert_json_snapshot!(value, @r###"
        {
          "archived_at": null,
          "assignee_id": null,
          "board_id": "0195f48b-d000-7000-8000-000000000013",
          "card_type_id": "0195f48b-d000-7000-8000-000000000012",
          "claim_fence": 3,
          "id": "0195f48b-d000-7000-8000-000000000010",
          "lease_expires_at": null,
          "lease_token": null,
          "list_changed_at": "2023-11-14T22:13:20Z",
          "list_id": "0195f48b-d000-7000-8000-000000000011",
          "prev_list_id": null,
          "version": 4
        }
        "###);
    }
}
