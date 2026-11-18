-- 0052 — #303: the governed retirement path for benefit profiles and tranches.
--
-- Both rows gain their terminal state and the reason that records why they left the map. The
-- vocabulary is extended, never replaced, and a retired row stays readable by id with its
-- transitions — retirement is never a delete.
--
-- A benefit profile that advances through change control also cites the approved change on its
-- own row (`change_ref`), which is what makes the mutual citation between the profile and the
-- change auditable from either side.

ALTER TABLE benefit_profile
    DROP CONSTRAINT IF EXISTS benefit_profile_status_check;
ALTER TABLE benefit_profile
    ADD CONSTRAINT benefit_profile_status_check
    CHECK (status IN ('draft', 'active', 'realized', 'disbenefit', 'closed', 'retired'));

ALTER TABLE benefit_profile
    ADD COLUMN retired_reason text NOT NULL DEFAULT '';
ALTER TABLE benefit_profile
    ADD COLUMN change_ref uuid;
ALTER TABLE benefit_profile
    ADD CONSTRAINT benefit_profile_change_fk
    FOREIGN KEY (change_ref) REFERENCES project_register_item (id);

ALTER TABLE programme_tranche
    DROP CONSTRAINT IF EXISTS programme_tranche_state_check;
ALTER TABLE programme_tranche
    ADD CONSTRAINT programme_tranche_state_check
    CHECK (state IN ('planned', 'authorised', 'in_review', 'closed', 'paused', 'stopped',
                     'retired'));

ALTER TABLE programme_tranche
    ADD COLUMN retired_reason text NOT NULL DEFAULT '';
