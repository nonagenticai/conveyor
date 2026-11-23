-- 0066_cadence_state.sql — `programme_cadence.state` is a closed set (SPEC_V2 §8.2; finding
-- v1-p82-2, conveyor#526).
--
-- The column was unconstrained text and `cadences.update` wrote the caller's string verbatim, so
-- any value could be stored and a cadence's history could be rewritten by a typo. The set below
-- is every value the code writes: `planned` (the column default), `awaiting_approval` and
-- `rejected` (the PI-plan gate), `materialised` (the tick's `materialise_pi_tx`), `closed` (a
-- retro close) and `active` (the MSP editor's existing correction). The legal moves between them
-- are the store's one transition table (`crates/store/src/spine.rs::cadence_move`); this CHECK is
-- the second lock, never the first.
--
-- Constraint only: the table's existing tenant policy and table-level grants cover it, so no RLS
-- or grant change is made here. Forward only: never edited after this slice closes (CONTRACT §2).

ALTER TABLE programme_cadence
    ADD CONSTRAINT programme_cadence_state_check
    CHECK (state IN ('planned', 'awaiting_approval', 'rejected', 'materialised', 'closed', 'active'));
