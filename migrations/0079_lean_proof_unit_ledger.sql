-- 0079_lean_proof_unit_ledger.sql — the unit-ledger certificate subject (issue #836).
--
-- A `unit_ledger` certificate stores one observed chunk of delivery units and the kernel's
-- verdict on it. The subject id is the deterministic chunk id the collector computes from the
-- corpus hash, so the same snapshot names the same subject and a re-run is a replay, never a
-- second certificate. The certificate's facts carry the observations; the model is
-- `docs/spec/lean/DeliveryModel.lean`.
--
-- Forward only: never edited after this slice closes (CONTRACT §2).

ALTER TABLE proof DROP CONSTRAINT proof_subject_type_check;
ALTER TABLE proof
    ADD CONSTRAINT proof_subject_type_check
        CHECK (subject_type IN ('card', 'run', 'wbs_node', 'release',
                                'programme', 'package', 'unit_ledger'));
