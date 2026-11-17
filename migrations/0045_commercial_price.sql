-- 0043_commercial_price.sql — what the work would cost on a commercial API.
--
-- The conveyor derives cost at read time (SPEC_V2 D52): `inference_call` stores tokens, never
-- money. This table is the price side of that derivation, keyed by the model the ledger records
-- (`coalesce(served_model, model)`), so a price change is a row edit and never a rewrite of
-- history.
--
-- It is a PLATFORM fact, not tenant data: the same commercial equivalents apply to every tenant.
-- No `tenant_id` and no RLS — the `platform_admin` precedent (0028).
--
-- ⛔ A model with no row is UNPRICED, never $0. The report counts unpriced calls separately so
-- "we have no price for this" cannot render as "this work was free".
--
-- The seed is the commercial fallback the inference-router's own provider catalog already
-- declares for `local-llm` (deepseek-v4-flash at 0.14 in / 0.28 out per 1M tokens; measured
-- 2026-11-16 in the deployment repo's providers-configmap.yaml). USD.
CREATE TABLE commercial_price (
    model              text PRIMARY KEY,
    -- The commercial model this price belongs to; the model key above is what the ledger records.
    equivalent         text NOT NULL,
    input_usd_per_1m   numeric(12,6) NOT NULL CHECK (input_usd_per_1m >= 0),
    output_usd_per_1m  numeric(12,6) NOT NULL CHECK (output_usd_per_1m >= 0),
    -- Where the number came from. A price without a source is a guess.
    source             text NOT NULL,
    updated_at         timestamptz NOT NULL DEFAULT now()
);

INSERT INTO commercial_price (model, equivalent, input_usd_per_1m, output_usd_per_1m, source)
VALUES (
    'local-llm',
    'deepseek-v4-flash',
    0.140000,
    0.280000,
    'inference-router provider catalog (deepseek-v4-flash), 2026-11-16'
);
