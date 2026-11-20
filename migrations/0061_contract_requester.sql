-- 0061_contract_requester.sql — who requested an architecture contract (SPEC_V2 §8.1; conveyor#508).
--
-- A contract is a gate object: the board approves it, and the actor that requested it must not be
-- the actor that approves it. The column is nullable because contracts created before this
-- migration have no recorded requester; they are flagged `requester_unknown` for a waiver
-- decision and are never backfilled with a guess.

ALTER TABLE architecture_contract
    ADD COLUMN requested_by uuid;

ALTER TABLE architecture_contract
    ADD CONSTRAINT architecture_contract_requested_by_fk
    FOREIGN KEY (requested_by) REFERENCES app_user (id);
