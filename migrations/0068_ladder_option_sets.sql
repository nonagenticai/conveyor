-- 0068_ladder_option_sets.sql — S21 step 10 (#656): the authored option set and `fire_when`.
--
-- SPEC_V2 §7.2: "Options are rows (`decision_option`), authored and versioned, never free text
-- from a prompt." Before this migration the only option rows were `decision_option`, written per
-- decision from the request body, so a caller chose the menu it answered from. This adds the
-- authored set the case is built from, and the policy's `fire_when`.
--
-- Nothing existing changes shape: `decision_option` keeps its meaning (what this decision chose
-- between) and is written from these rows from now on.

CREATE TABLE decision_option_set (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    kind       text NOT NULL,
    gate       text,
    version    bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT decision_option_set_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id)
        ON DELETE CASCADE,
    CONSTRAINT decision_option_set_kind_not_blank CHECK (btrim(kind) <> ''),
    -- the child's composite FK target, so a row can never point at another tenant's set
    CONSTRAINT decision_option_set_id_tenant UNIQUE (id, tenant_id)
);

-- One live set per case. A policy is unique per (kind, gate) too, and `gate` is nullable, so the
-- uniqueness is split rather than relying on NULL comparison semantics.
CREATE UNIQUE INDEX uq_decision_option_set_gated
    ON decision_option_set (tenant_id, kind, gate) WHERE gate IS NOT NULL;
CREATE UNIQUE INDEX uq_decision_option_set_ungated
    ON decision_option_set (tenant_id, kind) WHERE gate IS NULL;

ALTER TABLE decision_option_set ENABLE ROW LEVEL SECURITY;
ALTER TABLE decision_option_set FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON decision_option_set FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

CREATE TABLE decision_option_set_option (
    set_id     uuid NOT NULL,
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    key        text NOT NULL,
    label      text NOT NULL DEFAULT '',
    payload    jsonb NOT NULL DEFAULT '{}'::jsonb,
    position   int NOT NULL DEFAULT 0 CHECK (position >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (set_id, key),
    CONSTRAINT decision_option_set_option_set_fk
        FOREIGN KEY (set_id, tenant_id) REFERENCES decision_option_set (id, tenant_id)
        ON DELETE CASCADE,
    CONSTRAINT decision_option_set_option_key_not_blank CHECK (btrim(key) <> ''),
    -- SPEC_V2 §7.3's three outcomes: an option the answer can land on must say what it means.
    CONSTRAINT decision_option_set_option_outcome CHECK (
        payload -> 'outcome' IS NULL
        OR payload ->> 'outcome' IN ('accepted', 'refused', 'escalated'))
);

ALTER TABLE decision_option_set_option ENABLE ROW LEVEL SECURITY;
ALTER TABLE decision_option_set_option FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON decision_option_set_option FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- `fire_when` is a structured expression evaluated when a case resolves; NULL fires always.
-- An unparseable expression is refused at authoring time (the action validates it), not here.
ALTER TABLE decision_policy ADD COLUMN fire_when jsonb;
