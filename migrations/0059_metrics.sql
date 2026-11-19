-- 0059_metrics.sql — T11: the maturity threshold configuration and the reconciliation record
-- (SPEC_V2 §7.5, §11.9, §15.1 D37/D50/D52; issue #84).
--
-- ⛔ NO METRIC IS STORED (D37). Every value the `metrics.*` actions return is computed at read
-- time from the ledgers §7.5 names. This migration adds only what the rules need: the tenant's
-- versioned threshold configuration (D50) and the verdict the billing reconciliation records.
-- A stored level, a stored cost, a nightly roll-up or a metric snapshot would be the second
-- truth this slice exists to prevent.

-- ── 1. metric_threshold — the C0–C5 thresholds, versioned per level ──────────────────────
-- One row per (tenant, level, version). A threshold change is a new version, never an edit, so a
-- computed level always names the version it used and an old level can be re-derived exactly.
CREATE TABLE metric_threshold (
    id         uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id  uuid NOT NULL DEFAULT app_current_tenant(),
    level      text NOT NULL CHECK (level IN ('C0', 'C1', 'C2', 'C3', 'C4', 'C5')),
    version    bigint NOT NULL CHECK (version >= 1),
    metrics    jsonb NOT NULL CHECK (jsonb_typeof(metrics) = 'object'),
    note       text NOT NULL DEFAULT '',
    created_by uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT metric_threshold_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT metric_threshold_version UNIQUE (tenant_id, level, version)
);
CREATE INDEX metric_threshold_latest_idx
    ON metric_threshold (tenant_id, level, version DESC);
ALTER TABLE metric_threshold ENABLE ROW LEVEL SECURITY;
ALTER TABLE metric_threshold FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON metric_threshold FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());

-- ── 2. metric_reconciliation — the billing comparison, never an adjustment ──────────────
-- A match is recorded; a difference opens an incident card (P2.2) and its figures are kept; an
-- unreachable billing service is `unavailable` with the upstream reason, never a zero
-- difference. The shape constraint makes those three states the only representable ones.
CREATE TABLE metric_reconciliation (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    tenant_id        uuid NOT NULL DEFAULT app_current_tenant(),
    period_start     date NOT NULL,
    period_end       date NOT NULL,
    conveyor_usd     numeric(18, 6),
    billing_usd      numeric(18, 6),
    delta_usd        numeric(18, 6),
    state            text NOT NULL CHECK (state IN ('matched', 'difference', 'unavailable')),
    upstream_error   text,
    incident_card_id uuid,
    recorded_by      uuid,
    created_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT metric_reconciliation_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenant (id) ON DELETE CASCADE,
    CONSTRAINT metric_reconciliation_period CHECK (period_end >= period_start),
    CONSTRAINT metric_reconciliation_once UNIQUE (tenant_id, period_start, period_end),
    CONSTRAINT metric_reconciliation_incident_fk FOREIGN KEY (incident_card_id) REFERENCES card (id) ON DELETE SET NULL,
    CONSTRAINT metric_reconciliation_shape CHECK (
        (state = 'unavailable' AND conveyor_usd IS NULL AND billing_usd IS NULL
            AND delta_usd IS NULL AND upstream_error IS NOT NULL)
        OR (state = 'matched' AND conveyor_usd IS NOT NULL AND billing_usd IS NOT NULL
            AND delta_usd = 0 AND upstream_error IS NULL)
        OR (state = 'difference' AND conveyor_usd IS NOT NULL AND billing_usd IS NOT NULL
            AND delta_usd = conveyor_usd - billing_usd AND upstream_error IS NULL)
    )
);
CREATE INDEX metric_reconciliation_period_idx
    ON metric_reconciliation (tenant_id, period_end DESC);
ALTER TABLE metric_reconciliation ENABLE ROW LEVEL SECURITY;
ALTER TABLE metric_reconciliation FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON metric_reconciliation FOR ALL
    USING (tenant_id = app_current_tenant())
    WITH CHECK (tenant_id = app_current_tenant());
