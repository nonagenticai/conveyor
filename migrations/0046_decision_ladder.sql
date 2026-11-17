-- T2 — the decision ladder (SPEC_V2 §7, §10.3, §12 T2; issue #73).
--
-- T0 shipped the five decision tables as schema and contracts only
-- (0040_organisation.sql:216-360); no writer exists. This migration adds the columns the
-- ladder records on every decision, so the three rungs project ONE shape:
--
--   l0 rule · l1 laya · l3 human
--
-- L2 generation is deliberately not a rung: a generation call is an `inference_call` bound to
-- a draft artefact, never a decision row (§7.4). The rung CHECK therefore admits exactly three.
--
-- The version pins are the reproducibility contract (§7.2): a decision is reproducible from
-- its row, which means the row names the policy, the refinement and the laya config versions
-- it used — not whichever version happens to be current when the row is read.

ALTER TABLE decision
    ADD COLUMN rung                text,
    ADD COLUMN answer              text,
    ADD COLUMN option_set_version  bigint,
    ADD COLUMN policy_version      bigint,
    ADD COLUMN refinement_version  bigint,
    ADD COLUMN laya_config_version bigint,
    ADD COLUMN actor_kind          text,
    ADD COLUMN actor_id            uuid,
    ADD COLUMN approval_id         uuid;

-- Every decision carries its rung and the actor that made it. The backfill cannot find a row
-- (T0 shipped the tables with no writer); if one exists, the UPDATE labels it as the laya rung
-- it must have been, and the NOT NULL below keeps a future writer honest.
UPDATE decision SET rung = 'laya', actor_kind = 'system' WHERE rung IS NULL;

ALTER TABLE decision
    ALTER COLUMN rung SET NOT NULL,
    ALTER COLUMN actor_kind SET NOT NULL,
    ADD CONSTRAINT decision_rung_check
        CHECK (rung IN ('rule', 'laya', 'human')),
    ADD CONSTRAINT decision_actor_kind_check
        CHECK (actor_kind IN ('system', 'user', 'agent', 'service')),
    ADD CONSTRAINT decision_option_set_version_positive
        CHECK (option_set_version IS NULL OR option_set_version > 0),
    ADD CONSTRAINT decision_policy_version_positive
        CHECK (policy_version IS NULL OR policy_version > 0),
    ADD CONSTRAINT decision_refinement_version_positive
        CHECK (refinement_version IS NULL OR refinement_version > 0),
    ADD CONSTRAINT decision_laya_config_version_positive
        CHECK (laya_config_version IS NULL OR laya_config_version > 0),
    ADD CONSTRAINT decision_approval_fk
        FOREIGN KEY (approval_id) REFERENCES approval (id);

-- The two reads the console and the ladder's own audit make: by case kind, and by rung.
CREATE INDEX decision_kind_idx ON decision (tenant_id, kind, created_at DESC);
CREATE INDEX decision_rung_idx ON decision (tenant_id, rung, created_at DESC);

-- One refinement per policy is the tenant's current per-case configuration; publishing a new
-- one bumps the row's version, and every decision pins the version it read.
CREATE UNIQUE INDEX decision_policy_refinement_one_per_policy
    ON decision_policy_refinement (tenant_id, policy_id);

-- One policy per case: a second create for the same (kind, gate) is refused by the database as
-- well as by the store's read, so two concurrent creates cannot both win.
CREATE UNIQUE INDEX decision_policy_case
    ON decision_policy (tenant_id, kind, (COALESCE(gate, '')));
